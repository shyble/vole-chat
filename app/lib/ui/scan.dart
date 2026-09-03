import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

/// Point the camera at someone's code. Pops with the raw text, or null if they backed out.
class ScanPage extends StatefulWidget {
  const ScanPage({super.key});

  @override
  State<ScanPage> createState() => _ScanPageState();
}

class _ScanPageState extends State<ScanPage> {
  final _controller = MobileScannerController(formats: const [BarcodeFormat.qrCode]);
  bool _done = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _found(BarcodeCapture capture) {
    if (_done) return;
    final text = capture.barcodes.map((b) => b.rawValue).whereType<String>().firstOrNull;
    if (text == null || text.isEmpty) return;
    _done = true;
    Navigator.of(context).pop(text);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Scan a code')),
      body: Stack(
        alignment: Alignment.bottomCenter,
        children: [
          MobileScanner(
            controller: _controller,
            onDetect: _found,
            errorBuilder: (context, error) => Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text(
                  'The camera is not available: ${error.errorCode.name}.\n\n'
                  'You can paste their code instead — copy it from their You tab.',
                  textAlign: TextAlign.center,
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(20),
            child: Text(
              'Point at the code in their You tab',
              style: Theme.of(context).textTheme.titleMedium?.copyWith(color: Colors.white),
            ),
          ),
        ],
      ),
    );
  }
}
