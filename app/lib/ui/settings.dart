import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../app_state.dart';
import 'scan.dart';

/// Your identity: who you are to others, and the one thing to back up.
class SettingsPage extends StatelessWidget {
  const SettingsPage(this.state, {super.key});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final s = state;
    final me = s.identity!;
    final card = s.myCard();
    final t = Theme.of(context).textTheme;
    return AnimatedBuilder(
      animation: s,
      builder: (context, _) => ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(me.name, style: t.headlineSmall),
          Text(me.instanceId, style: t.bodySmall),
          const SizedBox(height: 20),
          _Section(
            title: 'Your code',
            body: card?.seals == true
                ? 'Show this to someone and they can reach you by who you are, wherever either of you is. '
                    'It carries all four of your public keys${card?.hub != null ? ' and your hub' : ''}, so once they scan it no hub can '
                    'read your messages or pass anyone else off as you.'
                : 'What another node pins to trust you. Safe to share.',
            qr: card?.text ?? me.keyString,
            copyLabel: 'Copy code',
            copyValue: card?.text ?? me.keyString,
          ),
          const SizedBox(height: 12),
          FilledButton.icon(
            onPressed: () async {
              final text = await Navigator.of(context).push<String>(
                MaterialPageRoute(builder: (_) => const ScanPage()),
              );
              if (text == null || !context.mounted) return;
              try {
                final note = await s.addCard(text);
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(note)));
                }
              } catch (e) {
                if (context.mounted) {
                  ScaffoldMessenger.of(context)
                      .showSnackBar(SnackBar(content: Text('Not a vole-chat code: $e')));
                }
              }
            },
            icon: const Icon(Icons.qr_code_scanner),
            label: const Text("Scan someone's code"),
          ),
          const SizedBox(height: 20),
          _Section(
            title: 'Backup',
            body: 'This text is your identity. Anyone who has it is you. Keep it somewhere private; '
                'restore it on a new device from the first screen.',
            qr: me.seedsB64,
            copyLabel: 'Copy backup',
            copyValue: me.seedsB64,
            danger: true,
          ),
          const SizedBox(height: 20),
          if (s.cards.isNotEmpty) ...[
            Text('Codes you scanned', style: t.titleMedium),
            const SizedBox(height: 6),
            Text(
              'Their keys are pinned here, so a hub carrying your messages cannot read them or '
              'pass anyone else off as them.',
              style: t.bodySmall,
            ),
            for (final id in s.cards.keys)
              ListTile(
                dense: true,
                leading: const Icon(Icons.verified_user_outlined),
                title: Text(s.contacts[id]?.name.isNotEmpty == true ? s.contacts[id]!.name : id),
                subtitle: Text(id, style: t.bodySmall),
                trailing: IconButton(
                  icon: const Icon(Icons.close),
                  tooltip: 'Forget this code',
                  onPressed: () => s.forgetCard(id),
                ),
              ),
            const SizedBox(height: 20),
          ],
          Text('People who may message you', style: t.titleMedium),
          const SizedBox(height: 6),
          if (s.consents.isEmpty)
            Text('Nobody yet. Accepting a request, or sending someone a message, adds them here.', style: t.bodyMedium),
          for (final id in s.consents)
            ListTile(
              dense: true,
              leading: const Icon(Icons.check_circle_outline),
              title: Text(s.contacts[id]?.name.isNotEmpty == true ? s.contacts[id]!.name : id),
              subtitle: Text(id, style: t.bodySmall),
            ),
          const SizedBox(height: 28),
          Text(
            'Messages are sealed to the recipient with X25519 + ML-KEM-768 and signed with Ed25519 + ML-DSA-65. '
            'A hub sees who wrote to whom and when, and nothing else. A message to someone who is away waits on '
            'this device, never on a hub.',
            style: t.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({
    required this.title,
    required this.body,
    required this.qr,
    required this.copyLabel,
    required this.copyValue,
    this.danger = false,
  });
  final String title;
  final String body;
  final String qr;
  final String copyLabel;
  final String copyValue;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final cs = Theme.of(context).colorScheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: t.titleMedium?.copyWith(color: danger ? cs.error : null)),
            const SizedBox(height: 4),
            Text(body, style: t.bodyMedium),
            const SizedBox(height: 12),
            Center(
              child: Container(
                color: Colors.white,
                padding: const EdgeInsets.all(8),
                child: QrImageView(data: qr, size: 200, version: QrVersions.auto),
              ),
            ),
            const SizedBox(height: 10),
            Align(
              alignment: Alignment.centerRight,
              child: OutlinedButton.icon(
                onPressed: () async {
                  await Clipboard.setData(ClipboardData(text: copyValue));
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$copyLabel: copied')));
                  }
                },
                icon: const Icon(Icons.copy),
                label: Text(copyLabel),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
