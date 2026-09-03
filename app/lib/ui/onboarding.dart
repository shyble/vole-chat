import 'package:flutter/material.dart';

import '../app_state.dart';

/// First run: pick a name and get an identity, or paste a backup to restore one.
class OnboardingPage extends StatefulWidget {
  const OnboardingPage(this.state, {super.key});
  final AppState state;

  @override
  State<OnboardingPage> createState() => _OnboardingPageState();
}

class _OnboardingPageState extends State<OnboardingPage> {
  final _name = TextEditingController();
  final _seeds = TextEditingController();
  bool _restoring = false;
  bool _busy = false;
  String? _error;

  Future<void> _go() async {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Pick a name — it is how others see you.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      if (_restoring) {
        await widget.state.restoreIdentity(name, _seeds.text);
      } else {
        await widget.state.createIdentity(name);
      }
    } catch (e) {
      setState(() => _error = _restoring ? 'That backup did not restore: $e' : '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    return Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 440),
          child: ListView(
            padding: const EdgeInsets.all(28),
            shrinkWrap: true,
            children: [
              Text('vole-chat', style: t.headlineLarge),
              const SizedBox(height: 6),
              Text(
                'End-to-end encrypted, post-quantum sealed, relayed through hubs that cannot read a word. '
                'Your identity is a key on this device. Nothing is stored on a server.',
                style: t.bodyMedium,
              ),
              const SizedBox(height: 28),
              TextField(
                controller: _name,
                decoration: const InputDecoration(labelText: 'Your name', border: OutlineInputBorder()),
                textInputAction: TextInputAction.done,
                onSubmitted: (_) => _go(),
              ),
              if (_restoring) ...[
                const SizedBox(height: 14),
                TextField(
                  controller: _seeds,
                  maxLines: 3,
                  decoration: const InputDecoration(
                    labelText: 'Backup (paste the text from Settings → Backup)',
                    border: OutlineInputBorder(),
                  ),
                ),
              ],
              if (_error != null) ...[
                const SizedBox(height: 12),
                Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
              ],
              const SizedBox(height: 20),
              FilledButton(
                onPressed: _busy ? null : _go,
                child: Text(_restoring ? 'Restore identity' : 'Create identity'),
              ),
              TextButton(
                onPressed: _busy ? null : () => setState(() => _restoring = !_restoring),
                child: Text(_restoring ? 'Start fresh instead' : 'I have a backup'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
