import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../app_state.dart';

class ChatPage extends StatefulWidget {
  const ChatPage(this.state, this.peerId, {super.key});
  final AppState state;
  final String peerId;

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final _input = TextEditingController();
  final _scroll = ScrollController();
  bool _sending = false;

  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty || _sending) return;
    _input.clear();
    setState(() => _sending = true);
    final note = await widget.state.send(widget.peerId, text);
    if (mounted) {
      setState(() => _sending = false);
      if (note != null) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(note)));
      _jump();
    }
  }

  void _jump() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) _scroll.jumpTo(_scroll.position.maxScrollExtent);
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    return AnimatedBuilder(
      animation: s,
      builder: (context, _) {
        final c = s.contacts[widget.peerId];
        final name = c?.name.isNotEmpty == true ? c!.name : widget.peerId.substring(0, 8);
        final msgs = s.messages(widget.peerId);
        final request = s.requests.contains(widget.peerId);
        final consented = s.consents.contains(widget.peerId);
        final notice = s.pending[widget.peerId];
        _jump();
        return Scaffold(
          appBar: AppBar(
            actions: [
              PopupMenuButton<String>(
                onSelected: (v) async {
                  if (v != 'remove') return;
                  final ok = await showDialog<bool>(
                    context: context,
                    builder: (ctx) => AlertDialog(
                      title: Text('Remove $name?'),
                      content: const Text(
                        'This deletes the conversation, forgets their code, and withdraws consent '
                        'each way. They would have to ask again, and so would you.',
                      ),
                      actions: [
                        TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
                        FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Remove')),
                      ],
                    ),
                  );
                  if (ok != true || !context.mounted) return;
                  await s.removeContact(widget.peerId);
                  if (context.mounted) Navigator.of(context).pop();
                },
                itemBuilder: (_) => const [
                  PopupMenuItem(value: 'remove', child: Text('Remove contact')),
                ],
              ),
            ],
            title: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(name),
                Text(
                  c?.isNode == true || c?.isCaller == true
                      ? (c!.connected ? 'direct · ${c.sealed ? 'sealed end to end' : 'signed'}' : 'direct · offline — held until it is back')
                      : (c?.connected == true ? 'online' : 'away — messages wait here until they are back'),
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
          body: Column(
            children: [
              if (request)
                _Banner(
                  icon: Icons.waving_hand_outlined,
                  text: '$name wants to chat with you.',
                  actions: [
                    TextButton(onPressed: () => s.deny(widget.peerId), child: const Text('Decline')),
                    FilledButton(onPressed: () => s.approve(widget.peerId), child: const Text('Accept')),
                  ],
                )
              else if (!consented && c != null && !c.isNode && !c.isCaller)
                _Banner(
                  icon: Icons.lock_outline,
                  text: 'You have not connected with $name yet. Sending a message asks them to accept you.',
                ),
              if (notice != null)
                _Banner(
                  icon: Icons.mark_chat_unread_outlined,
                  text: '$name tried to reach you ${notice.count == 1 ? 'once' : '${notice.count} times'} '
                      'while you were away (last ${_time(notice.last)}). The message is still on their device '
                      'and arrives when they are online.',
                ),
              Expanded(
                child: msgs.isEmpty
                    ? const Center(child: Text('No messages yet.'))
                    : ListView.builder(
                        controller: _scroll,
                        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                        itemCount: msgs.length,
                        itemBuilder: (_, i) => _Bubble(msgs[i]),
                      ),
              ),
              SafeArea(
                top: false,
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(12, 6, 12, 10),
                  child: Row(
                    children: [
                      Expanded(
                        child: TextField(
                          controller: _input,
                          minLines: 1,
                          maxLines: 5,
                          textInputAction: TextInputAction.send,
                          onSubmitted: (_) => _send(),
                          decoration: InputDecoration(
                            hintText: c?.reachable == false ? '$name cannot receive sealed chat' : 'Message $name',
                            border: const OutlineInputBorder(borderRadius: BorderRadius.all(Radius.circular(24))),
                            isDense: true,
                          ),
                          enabled: c?.reachable != false,
                        ),
                      ),
                      const SizedBox(width: 8),
                      IconButton.filled(
                        onPressed: _sending || c?.reachable == false ? null : _send,
                        icon: const Icon(Icons.send),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

String _time(int ms) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  final now = DateTime.now();
  final sameDay = d.year == now.year && d.month == now.month && d.day == now.day;
  return sameDay ? DateFormat.Hm().format(d) : DateFormat.MMMd().add_Hm().format(d);
}

class _Bubble extends StatelessWidget {
  const _Bubble(this.m);
  final Msg m;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    if (m.system) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Center(child: Text(m.text, style: Theme.of(context).textTheme.bodySmall)),
      );
    }
    return Align(
      alignment: m.out ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 3),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        constraints: const BoxConstraints(maxWidth: 420),
        decoration: BoxDecoration(
          color: m.out ? cs.primaryContainer : cs.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(16),
          border: m.held ? Border.all(color: cs.outline, style: BorderStyle.solid) : null,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Text(m.text),
            const SizedBox(height: 2),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (m.held) ...[
                  Icon(Icons.schedule, size: 12, color: cs.onSurfaceVariant),
                  const SizedBox(width: 3),
                  Text('held', style: TextStyle(fontSize: 11, color: cs.onSurfaceVariant)),
                  const SizedBox(width: 6),
                ],
                Text(_time(m.ts), style: TextStyle(fontSize: 11, color: cs.onSurfaceVariant)),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _Banner extends StatelessWidget {
  const _Banner({required this.icon, required this.text, this.actions = const []});
  final IconData icon;
  final String text;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    return MaterialBanner(
      leading: Icon(icon),
      content: Text(text),
      actions: actions.isEmpty ? const [SizedBox.shrink()] : actions,
    );
  }
}
