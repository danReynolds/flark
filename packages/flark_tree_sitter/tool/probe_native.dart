import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/src/backend_ffi.dart';

Future<CodeAnalyzer> createAnalyzer() async => CodeAnalyzer();

Future<CodeBackend> createBackend() async => createCodeBackend();
