import 'dart:convert';
import 'dart:io';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Where things live, and nothing else.
///
/// Secrets (the identity's seeds, pinned hub keys, consents) go to the platform keystore.
/// Transcripts, the outbox, the last roster snapshot and notices are JSON files in the app's
/// own support directory. Small UI state is in preferences.
class Store {
  final _secure = const FlutterSecureStorage(
    aOptions: AndroidOptions(encryptedSharedPreferences: true),
  );
  late final Directory _dir;
  late final SharedPreferences _prefs;

  Future<void> init() async {
    final base = await getApplicationSupportDirectory();
    _dir = Directory('${base.path}/vole-chat');
    await Directory('${_dir.path}/transcripts').create(recursive: true);
    _prefs = await SharedPreferences.getInstance();
  }

  // ── secrets ──
  Future<Map<String, dynamic>?> readIdentity() async => _readJsonSecure('identity');
  Future<void> writeIdentity(String name, String seedsB64) =>
      _secure.write(key: 'identity', value: jsonEncode({'name': name, 'seeds': seedsB64}));
  Future<void> clearIdentity() => _secure.delete(key: 'identity');

  Future<List<Map<String, dynamic>>> readHubs() async =>
      ((await _readJsonSecure('hubs') ?? const {})['list'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>();
  Future<void> writeHubs(List<Map<String, dynamic>> hubs) =>
      _secure.write(key: 'hubs', value: jsonEncode({'list': hubs}));

  Future<List<Map<String, dynamic>>> readNodes() async =>
      ((await _readJsonSecure('nodes') ?? const {})['list'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>();
  Future<void> writeNodes(List<Map<String, dynamic>> nodes) =>
      _secure.write(key: 'nodes', value: jsonEncode({'list': nodes}));

  Future<List<String>> readConsents() async =>
      ((await _readJsonSecure('consents') ?? const {})['list'] as List<dynamic>? ?? const [])
          .cast<String>();
  Future<void> writeConsents(List<String> ids) =>
      _secure.write(key: 'consents', value: jsonEncode({'list': ids}));

  Future<Map<String, dynamic>?> _readJsonSecure(String key) async {
    final raw = await _secure.read(key: key);
    if (raw == null) return null;
    try {
      return jsonDecode(raw) as Map<String, dynamic>;
    } catch (_) {
      return null;
    }
  }

  // ── files ──
  File _file(String name) => File('${_dir.path}/$name');

  Future<dynamic> _readJsonFile(String name) async {
    final f = _file(name);
    if (!await f.exists()) return null;
    try {
      return jsonDecode(await f.readAsString());
    } catch (_) {
      return null;
    }
  }

  Future<void> _writeJsonFile(String name, Object value) async {
    final f = _file(name);
    final tmp = File('${f.path}.tmp');
    await tmp.writeAsString(jsonEncode(value), flush: true);
    await tmp.rename(f.path);
  }

  Future<List<Map<String, dynamic>>> readContacts() async =>
      ((await _readJsonFile('contacts.json')) as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>();
  Future<void> writeContacts(List<Map<String, dynamic>> c) => _writeJsonFile('contacts.json', c);

  Future<List<Map<String, dynamic>>> readTranscript(String peerId) async =>
      ((await _readJsonFile('transcripts/$peerId.json')) as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>();
  Future<void> writeTranscript(String peerId, List<Map<String, dynamic>> msgs) =>
      _writeJsonFile('transcripts/$peerId.json', msgs);
  Future<List<String>> transcriptPeers() async {
    final d = Directory('${_dir.path}/transcripts');
    return [
      await for (final e in d.list())
        if (e is File && e.path.endsWith('.json')) e.uri.pathSegments.last.replaceAll('.json', ''),
    ];
  }

  Future<String> readOutbox() async =>
      (await _file('outbox.json').exists()) ? await _file('outbox.json').readAsString() : '{"entries":[]}';
  Future<void> writeOutbox(String json) async {
    final f = _file('outbox.json');
    final tmp = File('${f.path}.tmp');
    await tmp.writeAsString(json, flush: true);
    await tmp.rename(f.path);
  }

  Future<List<Map<String, dynamic>>> readPending() async =>
      ((await _readJsonFile('pending.json')) as List<dynamic>? ?? const []).cast<Map<String, dynamic>>();
  Future<void> writePending(List<Map<String, dynamic>> p) => _writeJsonFile('pending.json', p);

  Future<List<String>> readRequests() async =>
      ((await _readJsonFile('requests.json')) as List<dynamic>? ?? const []).cast<String>();
  Future<void> writeRequests(List<String> r) => _writeJsonFile('requests.json', r);

  // ── prefs ──
  String? get activeHub => _prefs.getString('activeHub');
  Future<void> setActiveHub(String? url) async =>
      url == null ? _prefs.remove('activeHub') : _prefs.setString('activeHub', url);

  Map<String, int> get unread {
    final raw = _prefs.getString('unread');
    if (raw == null) return {};
    try {
      return (jsonDecode(raw) as Map<String, dynamic>).map((k, v) => MapEntry(k, v as int));
    } catch (_) {
      return {};
    }
  }

  Future<void> setUnread(Map<String, int> m) => _prefs.setString('unread', jsonEncode(m));
}
