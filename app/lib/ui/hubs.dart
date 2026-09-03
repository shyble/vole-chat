import 'package:flutter/material.dart';

import '../app_state.dart';
import '../src/rust/api.dart' as rust;

/// Hubs you have joined, and nodes you reach directly. One hub is active at a time; every
/// paired node has its own link. The app is live on all of them while open.
class HubsPage extends StatefulWidget {
  const HubsPage(this.state, {super.key});
  final AppState state;

  @override
  State<HubsPage> createState() => _HubsPageState();
}

class _HubsPageState extends State<HubsPage> {
  final _hubUrl = TextEditingController(text: 'https://club.openvole.com/mesh');
  final _nodeUrl = TextEditingController();
  final _note = TextEditingController();
  bool _busy = false;
  String? _hubError;
  String? _nodeError;

  Future<void> _addHub() async {
    final url = _hubUrl.text.trim();
    if (url.isEmpty) return;
    setState(() {
      _busy = true;
      _hubError = null;
    });
    try {
      final hub = await widget.state.addHub(url);
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Joined ${hub.name}')));
    } catch (e) {
      setState(() => _hubError = _clean(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Fingerprint first, then ask. Nothing is trusted until the operator accepts on their side.
  Future<void> _addNode() async {
    final url = _nodeUrl.text.trim();
    if (url.isEmpty) return;
    setState(() {
      _busy = true;
      _nodeError = null;
    });
    try {
      final info = await widget.state.probeNode(url);
      if (!mounted) return;
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text(info.name ?? 'Unnamed node'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Instance ${info.instanceId}'),
              Text(info.postQuantum ? 'Post-quantum keys: yes' : 'Post-quantum keys: no (older node)'),
              const SizedBox(height: 10),
              const Text(
                'This key will be pinned on your phone. Ask the operator to accept the pair request in that '
                'agent\'s dashboard (VoleNet tab) or with "vole net pair".',
              ),
            ],
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
            FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Send pair request')),
          ],
        ),
      );
      if (ok != true) return;
      final note = _note.text.trim();
      await widget.state.addNode(url, info, note: note.isEmpty ? null : note);
      _nodeUrl.clear();
      _note.clear();
    } catch (e) {
      setState(() => _nodeError = _clean(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _clean(Object e) => e.toString().replaceFirst('Exception: ', '').replaceFirst('Bad state: ', '');

  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    final t = Theme.of(context).textTheme;
    return AnimatedBuilder(
      animation: s,
      builder: (context, _) => ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text('Hubs', style: t.headlineSmall),
          const SizedBox(height: 4),
          Text(
            'A hub relays sealed envelopes between members and cannot read them. Joining registers your key '
            'with it and pins its key on this device. Anyone can run one.',
            style: t.bodyMedium,
          ),
          const SizedBox(height: 12),
          for (final h in s.hubs)
            Card(
              child: ListTile(
                leading: Icon(
                  s.activeHub?.url == h.url && s.connected ? Icons.hub : Icons.hub_outlined,
                  color: s.activeHub?.url == h.url && s.connected ? Colors.green : null,
                ),
                title: Text(h.name),
                subtitle: Text(
                  '${h.url}\n${h.instanceId}${s.activeHub?.url == h.url ? '\n${s.status ?? ''}' : ''}',
                  style: t.bodySmall,
                ),
                isThreeLine: true,
                selected: s.activeHub?.url == h.url,
                onTap: () => s.selectHub(h),
                trailing: PopupMenuButton<String>(
                  onSelected: (v) {
                    if (v == 'use') s.selectHub(h);
                    if (v == 'reconnect') s.disconnect().then((_) => s.connect());
                    if (v == 'remove') s.removeHub(h);
                  },
                  itemBuilder: (_) => const [
                    PopupMenuItem(value: 'use', child: Text('Use this hub')),
                    PopupMenuItem(value: 'reconnect', child: Text('Reconnect')),
                    PopupMenuItem(value: 'remove', child: Text('Forget')),
                  ],
                ),
              ),
            ),
          const SizedBox(height: 8),
          TextField(
            controller: _hubUrl,
            decoration: const InputDecoration(labelText: 'Hub URL', border: OutlineInputBorder()),
            keyboardType: TextInputType.url,
            onSubmitted: (_) => _addHub(),
          ),
          if (_hubError != null) ...[
            const SizedBox(height: 8),
            Text(_hubError!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
          ],
          const SizedBox(height: 8),
          FilledButton.icon(
            onPressed: _busy ? null : _addHub,
            icon: const Icon(Icons.add),
            label: Text(_busy ? 'Working…' : 'Join hub'),
          ),
          const SizedBox(height: 28),
          Text('Direct nodes', style: t.headlineSmall),
          const SizedBox(height: 4),
          Text(
            'Your own agents, reached straight over their address with no hub in between — the way agents '
            'reach each other. You ask to be trusted; the operator accepts once; chat is sealed end to end.',
            style: t.bodyMedium,
          ),
          const SizedBox(height: 12),
          for (final n in s.nodes) _NodeCard(s, n),
          const SizedBox(height: 8),
          TextField(
            controller: _nodeUrl,
            decoration: const InputDecoration(
              labelText: 'Node URL (e.g. http://192.168.1.20:9700)',
              border: OutlineInputBorder(),
            ),
            keyboardType: TextInputType.url,
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _note,
            decoration: const InputDecoration(
              labelText: 'Note to the operator (optional)',
              border: OutlineInputBorder(),
            ),
            onSubmitted: (_) => _addNode(),
          ),
          if (_nodeError != null) ...[
            const SizedBox(height: 8),
            Text(_nodeError!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
          ],
          const SizedBox(height: 8),
          FilledButton.icon(
            onPressed: _busy ? null : _addNode,
            icon: const Icon(Icons.link),
            label: Text(_busy ? 'Working…' : 'Pair with node'),
          ),
        ],
      ),
    );
  }
}

class _NodeCard extends StatelessWidget {
  const _NodeCard(this.s, this.n);
  final AppState s;
  final Node n;

  @override
  Widget build(BuildContext context) {
    final c = s.contacts[n.instanceId];
    final live = c?.connected == true;
    final status = s.nodeStatus[n.instanceId] ?? (n.paired ? 'Paired' : 'Waiting for the operator to accept…');
    return Card(
      child: ListTile(
        leading: Icon(live ? Icons.link : Icons.link_off, color: live ? Colors.green : null),
        title: Text(n.name),
        subtitle: Text('${n.url}\n${n.instanceId}\n$status', style: Theme.of(context).textTheme.bodySmall),
        isThreeLine: true,
        trailing: PopupMenuButton<String>(
          onSelected: (v) {
            if (v == 'reconnect') s.connectNode(n);
            if (v == 'remove') s.removeNode(n);
          },
          itemBuilder: (_) => const [
            PopupMenuItem(value: 'reconnect', child: Text('Reconnect')),
            PopupMenuItem(value: 'remove', child: Text('Forget')),
          ],
        ),
      ),
    );
  }
}

// Keeps the rust import used for the fingerprint type even if the dialog changes shape.
typedef NodeFingerprint = rust.NodeInfoOut;
