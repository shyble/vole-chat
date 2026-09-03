import 'package:flutter/material.dart';

import '../app_state.dart';
import 'home.dart';
import 'onboarding.dart';

class VoleChatApp extends StatelessWidget {
  const VoleChatApp(this.state, {super.key});
  final AppState state;

  @override
  Widget build(BuildContext context) {
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
