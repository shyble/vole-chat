import 'dart:async';

import 'package:flutter/foundation.dart';

import 'src/rust/api.dart' as rust;
import 'store.dart';

class Hub {
  Hub({required this.url, required this.keyString, required this.instanceId, required this.name});
  final String url;
  final String keyString;
  final String instanceId;
  final String name;

  Map<String, dynamic> toJson() =>
      {'url': url, 'keyString': keyString, 'instanceId': instanceId, 'name': name};
  static Hub fromJson(Map<String, dynamic> j) => Hub(
        url: j['url'] as String,
        keyString: j['keyString'] as String,
        instanceId: j['instanceId'] as String,
        name: j['name'] as String? ?? 'hub',
      );
}

/// A node reached directly — your own agent, typically. Trusted by pairing, not by a hub.
class Node {
  Node({
    required this.url,
    required this.keyString,
    required this.instanceId,
    required this.name,
    this.paired = false,
  });
  final String url;
  final String keyString;
  final String instanceId;
  String name;
  bool paired;

  Map<String, dynamic> toJson() =>
      {'url': url, 'keyString': keyString, 'instanceId': instanceId, 'name': name, 'paired': paired};
  static Node fromJson(Map<String, dynamic> j) => Node(
        url: j['url'] as String,
        keyString: j['keyString'] as String,
        instanceId: j['instanceId'] as String,
        name: j['name'] as String? ?? 'node',
        paired: j['paired'] as bool? ?? false,
      );
}

class Contact {
  Contact({
    required this.instanceId,
    required this.name,
    required this.keyString,
    this.connected = false,
    this.reachable = false,
    this.isNode = false,
    this.isCaller = false,
    this.sealed = true,
  });
  final String instanceId;
  String name;
  String keyString;
  bool connected;
  bool reachable;
  /// Reached directly, not through a hub. No consent step: pairing was the consent.
  bool isNode;
  /// They dialled us rather than the other way round — we are the reachable one.
  bool isCaller;
  /// For a direct node: whether chat to it is sealed end to end (it announced sealing keys).
  bool sealed;

  Map<String, dynamic> toJson() => {
        'instanceId': instanceId,
        'name': name,
        'keyString': keyString,
        'reachable': reachable,
        'isNode': isNode,
        'isCaller': isCaller,
        'sealed': sealed,
      };
  static Contact fromJson(Map<String, dynamic> j) => Contact(
        instanceId: j['instanceId'] as String,
        name: j['name'] as String? ?? '',
        keyString: j['keyString'] as String? ?? '',
        reachable: j['reachable'] as bool? ?? false,
        isNode: j['isNode'] as bool? ?? false,
        isCaller: j['isCaller'] as bool? ?? false,
        sealed: j['sealed'] as bool? ?? true,
      );
}

class Msg {
  Msg({
    required this.id,
    required this.out,
    required this.text,
    required this.ts,
    this.held = false,
    this.ref,
    this.system = false,
  });
  final String id;
  final bool out;
  final String text;
  final int ts;
  bool held;
  String? ref;
  final bool system;

  Map<String, dynamic> toJson() => {
        'id': id,
        'out': out,
        'text': text,
        'ts': ts,
        if (held) 'held': true,
        if (ref != null) 'ref': ref,
        if (system) 'system': true,
      };
  static Msg fromJson(Map<String, dynamic> j) => Msg(
        id: j['id'] as String,
        out: j['out'] as bool? ?? false,
        text: j['text'] as String? ?? '',
        ts: j['ts'] as int? ?? 0,
        held: j['held'] as bool? ?? false,
        ref: j['ref'] as String?,
        system: j['system'] as bool? ?? false,
      );
}

class Notice {
  Notice({required this.from, required this.fromName, required this.count, required this.last});
  final String from;
  final String fromName;
  final int count;
  final int last;
  Map<String, dynamic> toJson() => {'from': from, 'fromName': fromName, 'count': count, 'last': last};
  static Notice fromJson(Map<String, dynamic> j) => Notice(
        from: j['from'] as String,
        fromName: j['fromName'] as String? ?? '',
        count: j['count'] as int? ?? 1,
        last: j['last'] as int? ?? 0,
      );
}

const _outboxTtlMs = 7 * 24 * 60 * 60 * 1000;

/// Everything the screens read, and every action they take. One hub session, plus one direct
/// session per paired node. Live while the app is open; closed cleanly when it is not.
class AppState extends ChangeNotifier {
  final Store store = Store();

  rust.IdentityInfo? identity;
  List<Hub> hubs = [];
  Hub? activeHub;
  List<Node> nodes = [];
  final Map<String, Contact> contacts = {};
  final Map<String, List<Msg>> transcripts = {};
  final Map<String, int> unread = {};
  final Set<String> consents = {};
  final Set<String> requests = {};
  final Map<String, Notice> pending = {};
  String outboxJson = '{"entries":[]}';

  rust.Session? _session;
  StreamSubscription<rust.SessionEvent>? _events;
  bool connected = false;
  String? status;
  String? viewing;
  Timer? _reconnect;

  /// Being reachable: another phone (or an agent) dials us instead.
  rust.Reachable? _reachable;
  StreamSubscription<rust.SessionEvent>? _reachableEvents;
  bool nearby = false;
  String? nearbyUrl;
  String? nearbyError;
  final Set<String> callerKeys = {};
  /// Cards we scanned, by instance id — their keys pinned by us, not vouched by a hub.
  final Map<String, String> cards = {};

  final Map<String, rust.NodeSession> _nodeSessions = {};
  final Map<String, StreamSubscription<rust.SessionEvent>> _nodeEvents = {};
  final Map<String, String> nodeStatus = {};
  Timer? _nodeRetry;
  bool _foreground = true;

  bool get hasIdentity => identity != null;

  Future<void> load() async {
    await store.init();
    final id = await store.readIdentity();
    if (id != null) {
      try {
        identity = rust.identityRestore(name: id['name'] as String, seedsB64: id['seeds'] as String);
      } catch (_) {
        identity = null;
      }
    }
    hubs = (await store.readHubs()).map(Hub.fromJson).toList();
    final active = store.activeHub;
    activeHub = hubs.where((h) => h.url == active).firstOrNull ?? hubs.firstOrNull;
    nodes = (await store.readNodes()).map(Node.fromJson).toList();
    for (final c in await store.readContacts()) {
      final contact = Contact.fromJson(c);
      contacts[contact.instanceId] = contact;
    }
    for (final peer in await store.transcriptPeers()) {
      transcripts[peer] = (await store.readTranscript(peer)).map(Msg.fromJson).toList();
    }
    unread.addAll(store.unread);
    consents.addAll(await store.readConsents());
    requests.addAll(await store.readRequests());
    for (final n in await store.readPending()) {
      final notice = Notice.fromJson(n);
      pending[notice.from] = notice;
    }
    outboxJson = await store.readOutbox();
    callerKeys.addAll(await store.readCallers());
    for (final text in await store.readCards()) {
      try {
        cards[rust.cardRead(text: text).instanceId] = text;
      } catch (_) {
        // not a card any more — drop it rather than fail the whole load
      }
    }
    nearby = store.nearby;
    notifyListeners();
    if (identity != null) connectAll();
  }

  // ── lifecycle ──

  /// The app came to the foreground: everything reconnects.
  void connectAll() {
    _foreground = true;
    if (identity == null) return;
    if (activeHub != null) unawaited(connect());
    for (final n in nodes) {
      unawaited(connectNode(n));
    }
    if (nearby) unawaited(startNearby());
  }

  /// The app is going away: close everything cleanly so the other side knows at once.
  void disconnectAll() {
    _foreground = false;
    _reconnect?.cancel();
    _nodeRetry?.cancel();
    unawaited(disconnect());
    for (final id in _nodeSessions.keys.toList()) {
      unawaited(_closeNode(id, 'app in background'));
    }
    _stopNearby();
  }

  // ── identity ──

  Future<void> createIdentity(String name) async {
    identity = rust.identityCreate(name: name.trim());
    await store.writeIdentity(identity!.name, identity!.seedsB64);
    notifyListeners();
  }

  Future<void> restoreIdentity(String name, String seedsB64) async {
    identity = rust.identityRestore(name: name.trim(), seedsB64: seedsB64.trim());
    await store.writeIdentity(identity!.name, identity!.seedsB64);
    notifyListeners();
  }

  // ── hubs ──

  Future<Hub> addHub(String url) async {
    final me = identity!;
    final j = await rust.hubJoin(url: url.trim(), name: me.name, seedsB64: me.seedsB64);
    if (j.pending) throw StateError('The hub queued your request for its operator to approve.');
    final hub = Hub(url: url.trim(), keyString: j.hubKeyString, instanceId: j.hubInstanceId, name: j.hubName);
    hubs.removeWhere((h) => h.url == hub.url);
    hubs.add(hub);
    await store.writeHubs(hubs.map((h) => h.toJson()).toList());
    if (activeHub == null) await selectHub(hub);
    notifyListeners();
    return hub;
  }

  Future<void> removeHub(Hub hub) async {
    hubs.removeWhere((h) => h.url == hub.url);
    await store.writeHubs(hubs.map((h) => h.toJson()).toList());
    if (activeHub?.url == hub.url) {
      await disconnect();
      activeHub = hubs.firstOrNull;
      await store.setActiveHub(activeHub?.url);
      if (activeHub != null) unawaited(connect());
    }
    notifyListeners();
  }

  Future<void> selectHub(Hub hub) async {
    if (activeHub?.url != hub.url) await disconnect();
    activeHub = hub;
    await store.setActiveHub(hub.url);
    notifyListeners();
    unawaited(connect());
  }

  // ── hub session ──

  Future<void> connect() async {
    final hub = activeHub;
    final me = identity;
    if (hub == null || me == null || _session != null || !_foreground) return;
    _reconnect?.cancel();
    status = 'Connecting to ${hub.name}…';
    notifyListeners();
    try {
      final s = await rust.sessionConnect(
        url: hub.url,
        hubKeyString: hub.keyString,
        name: me.name,
        seedsB64: me.seedsB64,
        consents: consents.toList(),
        cards: cards.values.toList(),
      );
      _session = s;
      _events = s.events().listen(_onEvent, onError: (_) => _lost('event stream failed'));
    } catch (e) {
      status = 'Could not connect: ${_short(e)}';
      connected = false;
      notifyListeners();
      _scheduleReconnect();
    }
  }

  Future<void> disconnect() async {
    _reconnect?.cancel();
    await _events?.cancel();
    _events = null;
    final s = _session;
    _session = null;
    if (s != null) await s.close();
    connected = false;
    status = null;
    for (final c in contacts.values) {
      if (!c.isNode) c.connected = false;
    }
    notifyListeners();
  }

  void _lost(String reason) {
    _events?.cancel();
    _events = null;
    _session = null;
    connected = false;
    status = 'Disconnected: $reason';
    for (final c in contacts.values) {
      if (!c.isNode) c.connected = false;
    }
    notifyListeners();
    _scheduleReconnect();
  }

  void _scheduleReconnect() {
    _reconnect?.cancel();
    if (!_foreground) return;
    _reconnect = Timer(const Duration(seconds: 10), () {
      if (_session == null && activeHub != null && _foreground) unawaited(connect());
    });
  }

  void _onEvent(rust.SessionEvent e) {
    switch (e) {
      case rust.SessionEvent_Connected(:final hubName):
        connected = true;
        status = 'Connected to $hubName';
        notifyListeners();
      case rust.SessionEvent_Disconnected(:final reason):
        _lost(reason);
      case rust.SessionEvent_Roster(:final members):
        _onRoster(members);
      case rust.SessionEvent_Chat(:final from, :final fromName, :final text, :final sentAt, :final id):
        _onChat(from, fromName, text, sentAt, id);
      case rust.SessionEvent_ConnectRequest(:final from, :final fromName, :final note):
        _ensureContact(from, fromName);
        requests.add(from);
        unawaited(store.writeRequests(requests.toList()));
        if (note != null && note.isNotEmpty) {
          _append(from, Msg(id: 'req-$from-${_now()}', out: false, text: note, ts: _now(), system: true));
        }
        _bump(from);
        notifyListeners();
      case rust.SessionEvent_ConnectAccepted(:final from, :final fromName):
        _ensureContact(from, fromName);
        _append(from, Msg(id: 'acc-${_now()}', out: false, text: '$fromName accepted your request', ts: _now(), system: true));
        notifyListeners();
      case rust.SessionEvent_ConnectDenied(:final from, :final fromName):
        _ensureContact(from, fromName);
        _append(from, Msg(id: 'den-${_now()}', out: false, text: '$fromName declined your request', ts: _now(), system: true));
        notifyListeners();
      case rust.SessionEvent_Pending(:final notices):
        for (final n in notices) {
          _ensureContact(n.from, n.fromName);
          pending[n.from] = Notice(from: n.from, fromName: n.fromName, count: n.count, last: n.last);
        }
        unawaited(store.writePending(pending.values.map((n) => n.toJson()).toList()));
        notifyListeners();
    }
  }

  void _onRoster(List<rust.MemberInfo> members) {
    final seen = <String>{};
    for (final m in members) {
      seen.add(m.instanceId);
      final c = contacts.putIfAbsent(
        m.instanceId,
        () => Contact(instanceId: m.instanceId, name: m.name, keyString: m.keyString),
      );
      if (c.isNode || c.isCaller) continue; // their own link decides whether they are here
      c.name = m.name;
      if (m.keyString.isNotEmpty) c.keyString = m.keyString;
      c.connected = m.connected;
      c.reachable = m.reachable;
    }
    for (final c in contacts.values) {
      if (!c.isNode && !c.isCaller && !seen.contains(c.instanceId)) c.connected = false;
    }
    unawaited(store.writeContacts(contacts.values.map((c) => c.toJson()).toList()));
    notifyListeners();
    unawaited(flushOutbox());
  }

  void _onChat(String from, String fromName, String text, int sentAt, String id) {
    _ensureContact(from, fromName);
    pending.remove(from);
    unawaited(store.writePending(pending.values.map((n) => n.toJson()).toList()));
    _append(from, Msg(id: id, out: false, text: text, ts: sentAt));
    if (viewing != from) _bump(from);
    notifyListeners();
  }

  // ── identity cards ──

  /// My card, as QR text. It names the active hub, so scanning it is enough to reach me.
  rust.CardOut? myCard() {
    final me = identity;
    if (me == null) return null;
    try {
      return rust.cardMine(name: me.name, seedsB64: me.seedsB64, hub: activeHub?.url);
    } catch (_) {
      return null;
    }
  }

  /// Read a scanned card without acting on it, so the UI can confirm first.
  rust.CardOut readCard(String text) => rust.cardRead(text: text);

  /// Add someone by their card: pin their keys, join the hub it names if we have none in
  /// common, and ask to connect. Returns a line for the UI.
  Future<String> addCard(String text) async {
    final card = rust.cardRead(text: text);
    if (card.instanceId == identity?.instanceId) return 'That is your own card.';
    cards[card.instanceId] = card.text;
    await store.writeCards(cards.values.toList());
    final c = contacts.putIfAbsent(
      card.instanceId,
      () => Contact(instanceId: card.instanceId, name: card.name, keyString: ''),
    );
    if (card.name.isNotEmpty) c.name = card.name;
    c.reachable = card.seals || c.reachable;
    unawaited(store.writeContacts(contacts.values.map((x) => x.toJson()).toList()));
    notifyListeners();

    // A card can name a hub, so scanning one is enough to go from nothing to chatting.
    final hub = card.hub;
    if (hub != null && !hubs.any((h) => h.url == hub)) {
      try {
        await addHub(hub);
      } catch (e) {
        return '${card.name} added, but their hub could not be joined: ${_short(e)}';
      }
    }
    // Reconnect so the pinned keys are in force for this session.
    if (activeHub != null) {
      await disconnect();
      await connect();
    }
    if (!consents.contains(card.instanceId)) {
      final note = await requestConnect(card.instanceId, note: 'scanned your card');
      if (note != null) return '${card.name} added. $note';
    }
    return card.seals
        ? '${card.name} added — their keys are pinned, so no hub can read or impersonate.'
        : '${card.name} added from an identity-only code; sealing keys still come from the hub.';
  }

  Future<void> forgetCard(String instanceId) async {
    cards.remove(instanceId);
    await store.writeCards(cards.values.toList());
    notifyListeners();
  }

  // ── being reachable ──

  /// Start (or stop) accepting callers on this network. Foreground only, like everything else.
  Future<void> setNearby(bool on) async {
    nearby = on;
    await store.setNearby(on);
    if (on) {
      await startNearby();
    } else {
      _stopNearby();
    }
    notifyListeners();
  }

  Future<void> startNearby() async {
    final me = identity;
    if (me == null || _reachable != null || !_foreground) return;
    nearbyError = null;
    try {
      final r = await rust.reachableStart(
        name: me.name,
        seedsB64: me.seedsB64,
        port: 0,
        accepted: callerKeys.toList(),
      );
      _reachable = r;
      nearbyUrl = r.url();
      if (nearbyUrl == null) nearbyError = 'No network address — join a Wi-Fi network first.';
      _reachableEvents = r.events().listen(_onReachableEvent, onError: (_) => _stopNearby());
      _syncCallers();
    } catch (e) {
      nearbyError = _short(e);
      nearbyUrl = null;
    }
    notifyListeners();
  }

  void _stopNearby() {
    _reachableEvents?.cancel();
    _reachableEvents = null;
    _reachable?.stop();
    _reachable = null;
    nearbyUrl = null;
    for (final c in contacts.values) {
      if (c.isCaller) c.connected = false;
    }
    notifyListeners();
  }

  void _onReachableEvent(rust.SessionEvent e) {
    switch (e) {
      case rust.SessionEvent_ConnectRequest(:final from, :final fromName, :final note):
        _ensureContact(from, fromName);
        contacts[from]?.isCaller = true;
        requests.add(from);
        unawaited(store.writeRequests(requests.toList()));
        if (note != null && note.isNotEmpty) {
          _append(from, Msg(id: 'req-$from-${_now()}', out: false, text: note, ts: _now(), system: true));
        }
        _bump(from);
        notifyListeners();
      case rust.SessionEvent_Chat(:final from, :final fromName, :final text, :final sentAt, :final id):
        _syncCallers();
        _onChat(from, fromName, text, sentAt, id);
      default:
        break;
    }
  }

  /// Mirror the listener's view of who is accepted and connected into contacts.
  void _syncCallers() {
    final r = _reachable;
    if (r == null) return;
    for (final c in r.callers()) {
      final contact = contacts.putIfAbsent(
        c.instanceId,
        () => Contact(instanceId: c.instanceId, name: c.name, keyString: ''),
      );
      contact.isCaller = true;
      contact.reachable = true;
      contact.connected = c.connected;
      if (c.name.isNotEmpty) contact.name = c.name;
    }
    unawaited(store.writeContacts(contacts.values.map((c) => c.toJson()).toList()));
  }

  // ── direct nodes ──

  /// Fingerprint a node by URL. Nothing is trusted yet.
  Future<rust.NodeInfoOut> probeNode(String url) => rust.nodeInfo(url: url.trim());

  /// Pin the node's key and ask its operator to trust us. Connects as soon as they do.
  Future<Node> addNode(String url, rust.NodeInfoOut info, {String? note}) async {
    final me = identity!;
    final node = Node(
      url: url.trim(),
      keyString: info.keyString,
      instanceId: info.instanceId,
      name: info.name ?? info.instanceId.substring(0, 8),
    );
    final p = await rust.nodePair(url: node.url, name: me.name, seedsB64: me.seedsB64, note: note);
    node.paired = p.alreadyTrusted;
    nodes.removeWhere((n) => n.instanceId == node.instanceId);
    nodes.add(node);
    await store.writeNodes(nodes.map((n) => n.toJson()).toList());
    final c = contacts.putIfAbsent(
      node.instanceId,
      () => Contact(instanceId: node.instanceId, name: node.name, keyString: node.keyString, isNode: true),
    );
    c.isNode = true;
    c.name = node.name;
    c.keyString = node.keyString;
    unawaited(store.writeContacts(contacts.values.map((c) => c.toJson()).toList()));
    nodeStatus[node.instanceId] = node.paired ? 'Connecting…' : 'Waiting for the operator to accept…';
    notifyListeners();
    unawaited(connectNode(node));
    return node;
  }

  Future<void> removeNode(Node node) async {
    await _closeNode(node.instanceId, 'forgotten');
    nodes.removeWhere((n) => n.instanceId == node.instanceId);
    nodeStatus.remove(node.instanceId);
    await store.writeNodes(nodes.map((n) => n.toJson()).toList());
    notifyListeners();
  }

  Future<void> connectNode(Node node) async {
    final me = identity;
    if (me == null || !_foreground || _nodeSessions.containsKey(node.instanceId)) return;
    nodeStatus[node.instanceId] = 'Connecting…';
    notifyListeners();
    try {
      final s = await rust.nodeConnect(
        url: node.url,
        nodeKeyString: node.keyString,
        name: me.name,
        seedsB64: me.seedsB64,
      );
      _nodeSessions[node.instanceId] = s;
      if (!node.paired) {
        node.paired = true;
        await store.writeNodes(nodes.map((n) => n.toJson()).toList());
      }
      final c = contacts.putIfAbsent(
        node.instanceId,
        () => Contact(instanceId: node.instanceId, name: node.name, keyString: node.keyString, isNode: true),
      );
      c.isNode = true;
      c.sealed = s.seals();
      c.reachable = true;
      _nodeEvents[node.instanceId] = s.events().listen(
        (e) => _onNodeEvent(node, e),
        onError: (_) => _nodeLost(node, 'event stream failed'),
      );
    } catch (e) {
      final msg = e.toString();
      if (msg.contains('not trusted')) {
        nodeStatus[node.instanceId] = 'Waiting for the operator to accept…';
      } else {
        nodeStatus[node.instanceId] = 'Could not connect: ${_short(e)}';
      }
      _scheduleNodeRetry();
    }
    notifyListeners();
  }

  void _onNodeEvent(Node node, rust.SessionEvent e) {
    switch (e) {
      case rust.SessionEvent_Connected(:final hubName):
        node.name = hubName;
        final c = contacts[node.instanceId];
        if (c != null) {
          c.connected = true;
          c.name = hubName;
        }
        nodeStatus[node.instanceId] = 'Connected';
        notifyListeners();
        unawaited(_flushNodeOutbox(node.instanceId));
      case rust.SessionEvent_Disconnected(:final reason):
        _nodeLost(node, reason);
      case rust.SessionEvent_Chat(:final from, :final fromName, :final text, :final sentAt, :final id):
        _onChat(from, fromName, text, sentAt, id);
      default:
        break;
    }
  }

  void _nodeLost(Node node, String reason) {
    _nodeEvents.remove(node.instanceId)?.cancel();
    _nodeSessions.remove(node.instanceId);
    contacts[node.instanceId]?.connected = false;
    nodeStatus[node.instanceId] = 'Disconnected: $reason';
    notifyListeners();
    _scheduleNodeRetry();
  }

  Future<void> _closeNode(String id, String reason) async {
    await _nodeEvents.remove(id)?.cancel();
    final s = _nodeSessions.remove(id);
    if (s != null) await s.close();
    contacts[id]?.connected = false;
    nodeStatus[id] = reason;
    notifyListeners();
  }

  void _scheduleNodeRetry() {
    _nodeRetry?.cancel();
    if (!_foreground) return;
    _nodeRetry = Timer(const Duration(seconds: 15), () {
      for (final n in nodes) {
        if (!_nodeSessions.containsKey(n.instanceId)) unawaited(connectNode(n));
      }
    });
  }

  /// Deliver what waited for a direct node, now that its link is up.
  Future<void> _flushNodeOutbox(String nodeId) async {
    final s = _nodeSessions[nodeId];
    if (s == null) return;
    for (final e in rust.outboxList(outboxJson: outboxJson).where((e) => e.to == nodeId)) {
      final v = s.sendChat(text: e.text, sentAt: e.sentAt);
      if (v is rust.SendVerdict_Delivered || v is rust.SendVerdict_Legacy) {
        outboxJson = rust.outboxRemove(outboxJson: outboxJson, ref: e.ref);
        for (final m in transcripts[nodeId] ?? const <Msg>[]) {
          if (m.ref == e.ref) m.held = false;
        }
      } else {
        break;
      }
    }
    await store.writeOutbox(outboxJson);
    final t = transcripts[nodeId];
    if (t != null) await store.writeTranscript(nodeId, t.map((m) => m.toJson()).toList());
    notifyListeners();
  }

  // ── actions ──

  /// Send, or hold. Returns a short status for the UI.
  Future<String?> send(String to, String text) async {
    final ts = _now();
    final contact = contacts[to];
    final node = _nodeSessions[to];
    if (node != null) {
      final v = node.sendChat(text: text, sentAt: ts);
      switch (v) {
        case rust.SendVerdict_Delivered() || rust.SendVerdict_Legacy():
          _append(to, Msg(id: 'out-$ts', out: true, text: text, ts: ts));
          notifyListeners();
          return null;
        case rust.SendVerdict_Held():
          _hold(to, contact?.name ?? to, text, ts);
          return 'Held until the link is back';
        case rust.SendVerdict_Refused(:final reason):
          _hold(to, contact?.name ?? to, text, ts);
          return 'Held: $reason';
      }
    }
    if (contact?.isCaller == true && _reachable?.sendChat(to: to, text: text, sentAt: ts) == true) {
      _append(to, Msg(id: 'out-$ts', out: true, text: text, ts: ts));
      notifyListeners();
      return null;
    }
    // A contact is an identity, not an address. When the direct route is down — an address
    // that moved, a phone that stopped listening — the same person is still reachable through
    // a hub you both use, so fall back to it rather than holding.
    final s = _session;
    final onHub = connected && s != null && contacts[to]?.keyString.isNotEmpty == true;
    final direct = contact?.isNode == true || contact?.isCaller == true;
    if (s == null || !connected || (direct && !onHub)) {
      _hold(to, contact?.name ?? to, text, ts);
      if (direct) return '${contact?.name ?? 'They'} are not reachable right now — held until they are';
      return 'Not connected — held until you are';
    }
    final v = await s.sendChat(to: to, text: text, sentAt: ts);
    switch (v) {
      case rust.SendVerdict_Delivered() || rust.SendVerdict_Legacy():
        _append(to, Msg(id: 'out-$ts', out: true, text: text, ts: ts));
        notifyListeners();
        return null;
      case rust.SendVerdict_Held():
        _hold(to, contact?.name ?? to, text, ts);
        return '${contact?.name ?? 'They'} are away — held here until they are back';
      case rust.SendVerdict_Refused(:final reason):
        return 'Not sent: $reason';
    }
  }

  void _hold(String to, String toName, String text, int ts) {
    final r = rust.outboxAdd(outboxJson: outboxJson, to: to, toName: toName, text: text, sentAt: ts);
    outboxJson = r.outboxJson;
    unawaited(store.writeOutbox(outboxJson));
    _append(to, Msg(id: 'out-$ts', out: true, text: text, ts: ts, held: true, ref: r.ref));
    notifyListeners();
  }

  Future<void> flushOutbox() async {
    final s = _session;
    if (s == null || rust.outboxList(outboxJson: outboxJson).isEmpty) return;
    final r = await s.flushOutbox(outboxJson: outboxJson, ttlMs: _outboxTtlMs);
    outboxJson = r.outboxJson;
    await store.writeOutbox(outboxJson);
    if (r.delivered.isEmpty && r.expired.isEmpty) return;
    for (final peerMsgs in transcripts.entries) {
      var changed = false;
      for (final m in peerMsgs.value) {
        if (m.held && m.ref != null && r.delivered.contains(m.ref)) {
          m.held = false;
          changed = true;
        }
      }
      if (changed) unawaited(store.writeTranscript(peerMsgs.key, peerMsgs.value.map((m) => m.toJson()).toList()));
    }
    notifyListeners();
  }

  Future<String?> requestConnect(String to, {String? note}) async {
    final s = _session;
    if (s == null) return 'Not connected';
    consents.add(to);
    await store.writeConsents(consents.toList());
    final v = await s.requestConnect(to: to, note: note);
    return switch (v) {
      rust.SendVerdict_Refused(:final reason) => 'Not sent: $reason',
      rust.SendVerdict_Held() => 'They are away — they will see it when they are back',
      _ => null,
    };
  }

  Future<void> approve(String id) async {
    requests.remove(id);
    await store.writeRequests(requests.toList());
    // A caller asked us directly: accepting is what opens their link, and their key is ours
    // to keep so they stay trusted next time.
    final key = _reachable?.accept(instanceId: id);
    if (key != null) {
      callerKeys.add(key);
      await store.writeCallers(callerKeys.toList());
      _syncCallers();
      notifyListeners();
      return;
    }
    consents.add(id);
    await store.writeConsents(consents.toList());
    _session?.accept(instanceId: id);
    await _session?.approve(to: id);
    notifyListeners();
  }

  Future<void> deny(String id) async {
    requests.remove(id);
    await store.writeRequests(requests.toList());
    if (contacts[id]?.isCaller == true) {
      _reachable?.deny(instanceId: id);
    } else {
      await _session?.deny(to: id);
    }
    notifyListeners();
  }

  void openChat(String id) {
    viewing = id;
    if (unread.remove(id) != null) unawaited(store.setUnread(unread));
    notifyListeners();
  }

  void closeChat() {
    viewing = null;
  }

  List<Msg> messages(String id) => transcripts[id] ?? const [];

  // ── helpers ──

  void _ensureContact(String id, String name) {
    final c = contacts.putIfAbsent(id, () => Contact(instanceId: id, name: name, keyString: ''));
    if (name.isNotEmpty) c.name = name;
  }

  void _append(String peer, Msg m) {
    final list = transcripts.putIfAbsent(peer, () => []);
    if (list.any((x) => x.id == m.id)) return;
    list.add(m);
    unawaited(store.writeTranscript(peer, list.map((x) => x.toJson()).toList()));
  }

  void _bump(String id) {
    unread[id] = (unread[id] ?? 0) + 1;
    unawaited(store.setUnread(unread));
  }

  int _now() => DateTime.now().millisecondsSinceEpoch;

  String _short(Object e) {
    final s = e.toString().replaceFirst('Exception: ', '');
    return s.length > 160 ? '${s.substring(0, 160)}…' : s;
  }

  @override
  void dispose() {
    _reconnect?.cancel();
    _nodeRetry?.cancel();
    _events?.cancel();
    _reachableEvents?.cancel();
    _reachable?.stop();
    for (final s in _nodeEvents.values) {
      s.cancel();
    }
    super.dispose();
  }
}
