import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';

void main() => runApp(const MaterialApp(home: NotePage()));

class NotePage extends StatelessWidget {
  const NotePage({super.key});
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('My note')),
    body: FlarkEditor(
      initialMarkdown: '# My note\n\nStart writing.',
      onChanged: (markdown) {
        /* Save or debounce in your application. */
      },
    ),
  );
}
