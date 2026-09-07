import 'package:flark_tree_sitter/highlight_worker.dart';

Future<CodeHighlightWorker> createWorker() => CodeHighlightWorker.start();
