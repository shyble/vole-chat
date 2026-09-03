import 'package:flutter/material.dart';

import 'app_state.dart';
import 'src/rust/frb_generated.dart';
import 'ui/root.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final state = AppState();
  await state.load();
  runApp(VoleChatApp(state));
}
