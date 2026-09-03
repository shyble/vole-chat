import 'package:flutter/material.dart';

import '../app_state.dart';
import 'home.dart';
import 'onboarding.dart';

class VoleChatApp extends StatefulWidget {
  const VoleChatApp(this.state, {super.key});
  final AppState state;

  @override
  State<VoleChatApp> createState() => _VoleChatAppState();
}

/// The app is live while it is open and closes its sessions cleanly when it is not — so the
/// hub sees a real close the moment the phone locks, agents' rosters stay accurate, and what
/// they send while we are away is held rather than written into a dead socket.
class _VoleChatAppState extends State<VoleChatApp> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    switch (state) {
      case AppLifecycleState.resumed:
        widget.state.connectAll();
      case AppLifecycleState.paused:
      case AppLifecycleState.detached:
        widget.state.disconnectAll();
      case AppLifecycleState.inactive:
      case AppLifecycleState.hidden:
        break;
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = widget.state;
    return MaterialApp(
      title: 'vole-chat',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(colorSchemeSeed: const Color(0xFF6B4FBB), useMaterial3: true),
      darkTheme: ThemeData(colorSchemeSeed: const Color(0xFF6B4FBB), brightness: Brightness.dark, useMaterial3: true),
      home: AnimatedBuilder(
        animation: state,
        builder: (_, _) => state.hasIdentity ? HomePage(state) : OnboardingPage(state),
      ),
    );
  }
}
