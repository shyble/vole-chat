// The Dart side of the bridge against a real OpenVole hub and member, on this machine.
//
//   cd core && cargo build            # produces the host dylib the test loads
//   cd app  && flutter test test/bridge_test.dart
//
// Runs on the host VM (not inside the sandboxed app), so it can spawn harness/hub.ts — OpenVole
// checked out beside this repo, harness deps installed — and drive the generated API through
// the same native library the app ships with: create an identity, join, connect, read the
// roster, chat both ways. If this passes, the app can talk to a hub.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vole_chat/src/rust/api.dart';
import 'package:vole_chat/src/rust/frb_generated.dart';

const hubPort = 19997;
const memberPort = 19998;

/// The repo root: walk up from wherever the test runner put us until core/Cargo.toml appears.
String repoRoot() {
  var d = Directory.current;
  while (true) {
    if (File('${d.path}/core/Cargo.toml').existsSync()) return d.path;
    if (d.parent.path == d.path) throw StateError('repo root not found from ${Directory.current.path}');
    d = d.parent;
  }
}

/// Subscribes immediately and keeps everything, so a waiter that arrives late still sees it.
class Replay<T> {
  Replay(Stream<T> source) {
    source.listen(_items.add);
  }
  final List<T> _items = [];

  Future<T> firstWhere(bool Function(T) pred, Duration within) async {
    final deadline = DateTime.now().add(within);
    var seen = 0;
    while (true) {
      for (; seen < _items.length; seen++) {
        if (pred(_items[seen])) return _items[seen];
      }
      if (DateTime.now().isAfter(deadline)) throw TimeoutException('nothing matched within $within', within);
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
  }
}

class Harness {
  Harness(this.proc, this.lines);
  final Process proc;
  final Replay<Map<String, dynamic>> lines;
  late Map<String, dynamic> ready;

  static Future<Harness> spawn() async {
    final repo = repoRoot();
    final tmp = await Directory.systemTemp.createTemp('vole-chat-bridge-');
    final proc = await Process.start(
      '$repo/harness/node_modules/.bin/tsx',
      ['$repo/harness/hub.ts', '$hubPort', '$memberPort', tmp.path],
      workingDirectory: '$repo/harness',
    );
    proc.stderr.transform(utf8.decoder).listen((_) {});
    final lines = Replay(proc.stdout.transform(utf8.decoder).transform(const LineSplitter()).map((l) {
      try {
        return jsonDecode(l) as Map<String, dynamic>;
      } catch (_) {
        return <String, dynamic>{};
      }
    }));
    final h = Harness(proc, lines);
    h.ready = await h.wait((v) => v['ev'] == 'ready', const Duration(seconds: 40));
    return h;
  }

  void cmd(Map<String, dynamic> c) => proc.stdin.writeln(jsonEncode(c));

  Future<Map<String, dynamic>> wait(bool Function(Map<String, dynamic>) pred, Duration within) =>
      lines.firstWhere(pred, within);

  Future<void> stop() async {
    cmd({'cmd': 'exit'});
    await Future<void>.delayed(const Duration(milliseconds: 800));
    proc.kill();
  }
}

void main() {
  setUpAll(() async {
    final repo = repoRoot();
    final dylib = '$repo/core/target/debug/librust_lib_vole_chat.dylib';
    final dbg = Directory('$repo/core/target/debug');
    expect(File(dylib).existsSync(), isTrue,
        reason: 'cwd=${Directory.current.path} repo=$repo dylib=$dylib '
            'debugDir=${dbg.existsSync() ? dbg.listSync().map((e) => e.uri.pathSegments.last).where((n) => n.contains('vole')).join(',') : 'missing'} '
            '— build the core first: cd core && cargo build');
    await RustLib.init(externalLibrary: ExternalLibrary.open(dylib));
  });

  test('a phone on a real hub, through the bridge', () async {
    final h = await Harness.spawn();
    try {
      final hubUrl = h.ready['hubUrl'] as String;
      final memberId = h.ready['memberId'] as String;

      // identity + join + pin
      final me = identityCreate(name: 'flutter-phone');
      expect(identityRestore(name: 'flutter-phone', seedsB64: me.seedsB64).instanceId, me.instanceId);
      expect(keyStringParse(keyString: me.keyString).postQuantum, isTrue);
      final join = await hubJoin(url: hubUrl, name: me.name, seedsB64: me.seedsB64);
      expect(join.pending, isFalse);
      expect(join.hubKeyString, h.ready['hubKey']);

      // connect, events, roster
      final session = await sessionConnect(
        url: hubUrl,
        hubKeyString: join.hubKeyString,
        name: me.name,
        seedsB64: me.seedsB64,
        consents: [memberId],
        cards: [],
      );
      final events = Replay(session.events());
      await events.firstWhere((e) => e is SessionEvent_Connected, const Duration(seconds: 10));
      await events.firstWhere(
          (e) => e is SessionEvent_Roster && e.members.any((m) => m.instanceId == memberId && m.connected),
          const Duration(seconds: 20));
      final peer = session.roster().firstWhere((m) => m.instanceId == memberId);
      expect(peer.name, 'peer-b');
      expect(peer.reachable, isTrue);

      // us → them
      final v = await session.sendChat(to: memberId, text: 'hello from flutter');
      expect(v, isA<SendVerdict_Delivered>());
      final got = await h.wait(
          (x) => x['ev'] == 'volenet:chat' && x['text'] == 'hello from flutter', const Duration(seconds: 15));
      expect(got['fromName'], 'flutter-phone');

      // them → us
      h.cmd({'cmd': 'send', 'to': me.instanceId, 'text': 'hi flutter'});
      final chat = await events.firstWhere((e) => e is SessionEvent_Chat, const Duration(seconds: 15)) as SessionEvent_Chat;
      expect(chat.from, memberId);
      expect(chat.fromName, 'peer-b');
      expect(chat.text, 'hi flutter');

      // outbox helpers round-trip through the bridge
      final add = outboxAdd(outboxJson: '{"entries":[]}', to: memberId, toName: 'peer-b', text: 'later', sentAt: 1);
      expect(outboxList(outboxJson: add.outboxJson).single.ref, add.ref);
      expect(outboxList(outboxJson: outboxRemove(outboxJson: add.outboxJson, ref: add.ref)), isEmpty);

      await session.close();
    } finally {
      await h.stop();
    }
  });

  test('being reachable, and another identity dialling us, through the bridge', () async {
    final alice = identityCreate(name: 'alice-dart');
    final bob = identityCreate(name: 'bob-dart');

    final me = await reachableStart(name: alice.name, seedsB64: alice.seedsB64, port: 0, accepted: []);
    expect(me.isListening(), isTrue);
    expect(me.port(), greaterThan(0));
    final url = 'http://127.0.0.1:${me.port()}';
    final events = Replay(me.events());

    // Bob fingerprints Alice and asks. Nothing is trusted by connecting.
    final info = await nodeInfo(url: url);
    expect(info.instanceId, alice.instanceId);
    await expectLater(
      nodeConnect(url: url, nodeKeyString: info.keyString, name: bob.name, seedsB64: bob.seedsB64),
      throwsA(predicate((e) => e.toString().contains('not trusted'))),
    );
    final pair = await nodePair(url: url, name: bob.name, seedsB64: bob.seedsB64, note: 'over here');
    expect(pair.queued, isTrue);
    final req = await events.firstWhere((e) => e is SessionEvent_ConnectRequest, const Duration(seconds: 10))
        as SessionEvent_ConnectRequest;
    expect(req.fromName, 'bob-dart');
    expect(req.note, 'over here');
    expect(me.requests().single.instanceId, bob.instanceId);

    // Alice accepts, keeps his key, and the link opens.
    expect(me.accept(instanceId: bob.instanceId), bob.keyString);
    expect(me.requests(), isEmpty);
    final link = await nodeConnect(url: url, nodeKeyString: info.keyString, name: bob.name, seedsB64: bob.seedsB64);
    final bobEvents = Replay(link.events());
    await bobEvents.firstWhere((e) => e is SessionEvent_Connected, const Duration(seconds: 10));

    // Both directions.
    expect(link.sendChat(text: 'hi alice'), isA<SendVerdict_Delivered>());
    final in1 = await events.firstWhere((e) => e is SessionEvent_Chat, const Duration(seconds: 10)) as SessionEvent_Chat;
    expect(in1.text, 'hi alice');
    expect(me.callers().single.connected, isTrue);
    expect(me.sendChat(to: bob.instanceId, text: 'hi bob'), isTrue);
    final in2 = await bobEvents.firstWhere((e) => e is SessionEvent_Chat, const Duration(seconds: 10)) as SessionEvent_Chat;
    expect(in2.text, 'hi bob');

    await link.close();
    me.stop();
    expect(me.isListening(), isFalse);
  });

  test('straight to a node, through the bridge', () async {
    final h = await Harness.spawn();
    try {
      final url = h.ready['memberUrl'] as String;
      final nodeId = h.ready['memberId'] as String;
      final me = identityCreate(name: 'flutter-direct');

      final info = await nodeInfo(url: url);
      expect(info.instanceId, nodeId);
      expect(info.name, 'peer-b');

      // Refused until paired, with a message the app keys on.
      await expectLater(
        nodeConnect(url: url, nodeKeyString: info.keyString, name: me.name, seedsB64: me.seedsB64),
        throwsA(predicate((e) => e.toString().contains('not trusted'))),
      );
      final pair = await nodePair(url: url, name: me.name, seedsB64: me.seedsB64, note: 'phone');
      expect(pair.queued, isTrue);
      await h.wait((x) => x['ev'] == 'volenet:pair:request' && x['fromName'] == 'flutter-direct', const Duration(seconds: 15));
      h.cmd({'cmd': 'pair-accept', 'to': 'flutter-direct'});
      await h.wait((x) => x['ev'] == 'pair-accept-result', const Duration(seconds: 10));

      final node = await nodeConnect(url: url, nodeKeyString: info.keyString, name: me.name, seedsB64: me.seedsB64);
      final events = Replay(node.events());
      await events.firstWhere((e) => e is SessionEvent_Connected, const Duration(seconds: 10));
      expect(node.seals(), isTrue);
      expect(node.sendChat(text: 'direct from flutter'), isA<SendVerdict_Delivered>());
      final got = await h.wait((x) => x['ev'] == 'volenet:chat' && x['text'] == 'direct from flutter', const Duration(seconds: 15));
      expect(got['fromName'], 'flutter-direct');
      h.cmd({'cmd': 'send', 'to': me.instanceId, 'text': 'direct back'});
      final chat = await events.firstWhere((e) => e is SessionEvent_Chat, const Duration(seconds: 15)) as SessionEvent_Chat;
      expect(chat.text, 'direct back');
      await node.close();
    } finally {
      await h.stop();
    }
  });
}
