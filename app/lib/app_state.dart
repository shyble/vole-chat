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

class Contact {
  Contact({
    required this.instanceId,
    required this.name,
    required this.keyString,
    this.connected = false,
    this.reachable = false,
  });
  final String instanceId;
  String name;
  String keyString;
  bool connected;
  bool reachable;

  Map<String, dynamic> toJson() =>
      {'instanceId': instanceId, 'name': name, 'keyString': keyString, 'reachable': reachable};
  static Contact fromJson(Map<String, dynamic> j) => Contact(
        instanceId: j['instanceId'] as String,
        name: j['name'] as String? ?? '',
        keyString: j['keyString'] as String? ?? '',
        reachable: j['reachable'] as bool? ?? false,
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

/// Everything the screens read, and every action they take. One hub session at a time.
class AppState extends ChangeNotifier {
  final Store store = Store();

  rust.IdentityInfo? identity;
  List<Hub> hubs = [];
  Hub? activeHub;
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
    notifyListeners();
    if (identity != null && activeHub != null) unawaited(connect());
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

  // ── session ──

  Future<void> connect() async {
    final hub = activeHub;
    final me = identity;
    if (hub == null || me == null || _session != null) return;
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
      c.connected = false;
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
      c.connected = false;
    }
    notifyListeners();
    _scheduleReconnect();
  }

  void _scheduleReconnect() {
    _reconnect?.cancel();
    _reconnect = Timer(const Duration(seconds: 10), () {
      if (_session == null && activeHub != null) unawaited(connect());
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
        if (note != null && note.isNotEmpty) _append(from, Msg(id: 'req-$from-${DateTime.now().millisecondsSinceEpoch}', out: false, text: note, ts: _now(), system: true));
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
      c.name = m.name;
      if (m.keyString.isNotEmpty) c.keyString = m.keyString;
      c.connected = m.connected;
      c.reachable = m.reachable;
    }
    for (final c in contacts.values) {
      if (!seen.contains(c.instanceId)) c.connected = false;
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

  // ── actions ──

  /// Send, or hold. Returns a short status for the UI.
  Future<String?> send(String to, String text) async {
    final s = _session;
    final ts = _now();
    final contact = contacts[to];
    if (s == null || !connected) {
      _hold(to, contact?.name ?? to, text, ts);
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
    consents.add(id);
    requests.remove(id);
    await store.writeConsents(consents.toList());
    await store.writeRequests(requests.toList());
    _session?.accept(instanceId: id);
    await _session?.approve(to: id);
    notifyListeners();
  }

  Future<void> deny(String id) async {
    requests.remove(id);
    await store.writeRequests(requests.toList());
    await _session?.deny(to: id);
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
    final s = e.toString();
    return s.length > 160 ? '${s.substring(0, 160)}…' : s;
  }

  @override
  void dispose() {
    _reconnect?.cancel();
    _events?.cancel();
    super.dispose();
  }
}
