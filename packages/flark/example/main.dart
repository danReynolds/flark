// Headless editing with the Flark kernel. Apps with a UI use flark_flutter or
// flark_fleury, which own input and painting over this same session.
import 'package:flark/flark.dart';
import 'package:flark/session.dart';

Future<void> main() async {
  final session = FlarkSession(markdown: 'Plain ');
  await session.ready;

  // Commands edit the Markdown source the way a keyboard would.
  session.command(const SetSelection(6, 6));
  session.command(const ToggleStyle(Style.strong));
  for (final character in 'bold words'.split('')) {
    session.command(InsertText(character));
  }
  print(session.state.markdown); // Plain **bold words**

  // The parser's render model is available to hosts and tools. Whoever
  // creates a parser disposes it; the session disposes the one it loaded.
  final parser = createParseBackend();
  final model = parser.parse(session.state.markdown);
  print('${model.blockCount} blocks, ${model.runCount} runs');
  parser.dispose();
  session.dispose();
}
