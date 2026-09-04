import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app_state.dart';

/// What the app actually believes right now, in one screen you can copy and send.
///
/// Chat either arrives or it does not, and when it does not the interesting part is invisible:
/// whether a session is up, whether the hub has named the other person, whether anything is
/// waiting and why it last failed. Guessing at that from the outside costs a build and a
/// retest each time.
class DiagnosticsPage extends StatefulWidget {
  const DiagnosticsPage(this.state, {super.key});
  final AppState state;

  @override
  State<DiagnosticsPage> createState() => _DiagnosticsPageState();
}

class _DiagnosticsPageState extends State<DiagnosticsPage> {
  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    return AnimatedBuilder(
      animation: s,
      builder: (context, _) {
        final report = s.diagnostics();
        return Scaffold(
          appBar: AppBar(
            title: const Text('Diagnostics'),
            actions: [
              IconButton(
                icon: const Icon(Icons.refresh),
                tooltip: 'Refresh',
                onPressed: () => setState(() {}),
              ),
              IconButton(
                icon: const Icon(Icons.copy),
                tooltip: 'Copy',
                onPressed: () async {
                  await Clipboard.setData(ClipboardData(text: report));
                  if (context.mounted) {
                    ScaffoldMessenger.of(context)
                        .showSnackBar(const SnackBar(content: Text('Diagnostics copied')));
                  }
                },
              ),
            ],
          ),
          body: SingleChildScrollView(
            padding: const EdgeInsets.all(16),
            child: SelectableText(report, style: const TextStyle(fontFamily: 'Menlo', fontSize: 12)),
          ),
        );
      },
    );
  }
}
