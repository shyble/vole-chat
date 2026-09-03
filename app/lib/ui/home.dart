import 'package:flutter/material.dart';

import '../app_state.dart';
import 'chat.dart';
import 'hubs.dart';
import 'settings.dart';

class HomePage extends StatefulWidget {
  const HomePage(this.state, {super.key});
  final AppState state;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  int _tab = 0;

  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    final pages = [ChatsTab(s), HubsPage(s), SettingsPage(s)];
    return Scaffold(
      body: SafeArea(child: pages[_tab]),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: (i) => setState(() => _tab = i),
        destinations: [
          NavigationDestination(
            icon: Badge(
              isLabelVisible: s.unread.isNotEmpty || s.requests.isNotEmpty,
              label: Text('${s.unread.values.fold(0, (a, b) => a + b) + s.requests.length}'),
              child: const Icon(Icons.chat_bubble_outline),
            ),
            selectedIcon: const Icon(Icons.chat_bubble),
            label: 'Chats',
          ),
          NavigationDestination(
            icon: Icon(s.connected ? Icons.hub : Icons.hub_outlined),
            label: 'Hubs',
          ),
          const NavigationDestination(icon: Icon(Icons.person_outline), selectedIcon: Icon(Icons.person), label: 'You'),
        ],
      ),
    );
  }
}

/// Everyone on the hub, plus anyone you have a transcript with. Requests and unread first.
class ChatsTab extends StatelessWidget {
  const ChatsTab(this.state, {super.key});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final s = state;
    final ids = <String>{...s.contacts.keys, ...s.transcripts.keys}.toList();
    int lastTs(String id) => s.transcripts[id]?.lastOrNull?.ts ?? 0;
    ids.sort((a, b) {
      final ra = s.requests.contains(a) ? 1 : 0, rb = s.requests.contains(b) ? 1 : 0;
      if (ra != rb) return rb - ra;
      final ua = s.unread[a] ?? 0, ub = s.unread[b] ?? 0;
      if ((ua > 0) != (ub > 0)) return ub - ua;
      final ta = lastTs(a), tb = lastTs(b);
      if (ta != tb) return tb - ta;
      final ca = s.contacts[a]?.connected == true ? 1 : 0, cb = s.contacts[b]?.connected == true ? 1 : 0;
      return cb - ca;
    });

    return Column(
      children: [
        ListTile(
          title: Text(s.activeHub?.name ?? 'No hub yet', style: Theme.of(context).textTheme.titleMedium),
          subtitle: Text(s.status ?? (s.activeHub == null ? 'Add a hub under Hubs to see who is around.' : '')),
          trailing: Icon(
            s.connected ? Icons.circle : Icons.circle_outlined,
            size: 14,
            color: s.connected ? Colors.green : Theme.of(context).disabledColor,
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: ids.isEmpty
              ? const Center(child: Text('Nobody here yet.'))
              : ListView.builder(
                  itemCount: ids.length,
                  itemBuilder: (_, i) => _ContactTile(s, ids[i]),
                ),
        ),
      ],
    );
  }
}

class _ContactTile extends StatelessWidget {
  const _ContactTile(this.s, this.id);
  final AppState s;
  final String id;

  @override
  Widget build(BuildContext context) {
    final c = s.contacts[id];
    final name = c?.name.isNotEmpty == true ? c!.name : id.substring(0, 8);
    final last = s.transcripts[id]?.lastOrNull;
    final unread = s.unread[id] ?? 0;
    final request = s.requests.contains(id);
    final held = s.transcripts[id]?.any((m) => m.held) ?? false;
    final notice = s.pending[id];
    String subtitle;
    if (request) {
      subtitle = 'wants to chat with you';
    } else if (notice != null) {
      subtitle = 'tried to reach you ${notice.count == 1 ? 'once' : '${notice.count} times'} while you were away';
    } else if (last != null) {
      subtitle = (last.out ? 'You: ' : '') + last.text;
    } else if (c?.isNode == true) {
      subtitle = c!.connected ? 'direct · online' : 'direct · offline';
    } else {
      subtitle = c?.connected == true ? 'online' : 'away';
    }
    return ListTile(
      leading: CircleAvatar(
        backgroundColor: c?.connected == true ? Colors.green.shade600 : Theme.of(context).colorScheme.surfaceContainerHighest,
        foregroundColor: c?.connected == true ? Colors.white : null,
        child: Text(name.isNotEmpty ? name[0].toUpperCase() : '?'),
      ),
      title: Text(name),
      subtitle: Text(subtitle, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (held) const Icon(Icons.schedule, size: 18),
          if (request || unread > 0) ...[
            const SizedBox(width: 6),
            Badge(label: Text(request ? '!' : '$unread')),
          ],
        ],
      ),
      onTap: () {
        s.openChat(id);
        Navigator.of(context)
            .push(MaterialPageRoute(builder: (_) => ChatPage(s, id)))
            .then((_) => s.closeChat());
      },
    );
  }
}
