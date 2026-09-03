import 'package:flutter/material.dart';

import '../app_state.dart';

/// Hubs you have joined. One is active at a time; the app is live on it while open.
class HubsPage extends StatefulWidget {
  const HubsPage(this.state, {super.key});
  final AppState state;

  @override
  State<HubsPage> createState() => _HubsPageState();
}

class _HubsPageState extends State<HubsPage> {
  final _url = TextEditingController(text: 'https://club.openvole.com/mesh');
  bool _busy = false;
  String? _error;

  Future<void> _add() async {
    final url = _url.text.trim();
    if (url.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final hub = await widget.state.addHub(url);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Joined ${hub.name}')));
      }
    } catch (e) {
      setState(() => _error = e.toString().replaceFirst('Exception: ', '').replaceFirst('Bad state: ', ''));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    return AnimatedBuilder(
      animation: s,
      builder: (context, _) => ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text('Hubs', style: Theme.of(context).textTheme.headlineSmall),
          const SizedBox(height: 4),
          Text(
            'A hub relays sealed envelopes between members and cannot read them. Joining registers your key '
            'with it and pins its key on this device. Anyone can run one.',
            style: Theme.of(context).textTheme.bodyMedium,
          ),
          const SizedBox(height: 16),
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
                  style: Theme.of(context).textTheme.bodySmall,
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
          const SizedBox(height: 16),
          Text('Join a hub', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          TextField(
            controller: _url,
            decoration: const InputDecoration(labelText: 'Hub URL', border: OutlineInputBorder()),
            keyboardType: TextInputType.url,
            onSubmitted: (_) => _add(),
          ),
          if (_error != null) ...[
            const SizedBox(height: 8),
            Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
          ],
          const SizedBox(height: 10),
          FilledButton.icon(
            onPressed: _busy ? null : _add,
            icon: const Icon(Icons.add),
            label: Text(_busy ? 'Joining…' : 'Join'),
          ),
        ],
      ),
    );
  }
}
