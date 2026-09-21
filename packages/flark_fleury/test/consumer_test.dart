import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test('public toolbar state, save routing and remount', () async {
    final c = FlarkController(markdown: 'hello');
    await c.ready;
    final tester = FleuryTester(viewportSize: const CellSize(100, 30));
    final saved = <String>[];
    void mount() => tester.pumpWidget(
      FleuryApp(
        title: 'Consumer',
        home: FlarkEditor(controller: c, onChanged: saved.add),
      ),
    );
    mount();
    tester.render();
    c.selectAll();
    tester.render();
    final press = await tester.invokeSemanticAction(
      SemanticAction.activate,
      role: SemanticRole.button,
      label: 'Bold',
    );
    expect(press.completed, isTrue);
    tester.render();
    expect(c.markdown, '**hello**');
    expect(c.state.styles.bold.isOn, isTrue);
    expect(saved, ['**hello**']);
    c.loadMarkdown('fetched');
    expect(saved.length, 1);
    tester.pumpWidget(const Text('unmounted'));
    tester.render();
    c.replaceMarkdown('offline');
    mount();
    tester.render();
    expect(c.markdown, 'offline');
    expect(saved.length, 1);
    tester.dispose();
    c.dispose();
    c.dispose();
  });
  test('dedicated reader renders and reacts', () async {
    final tester = FleuryTester(viewportSize: const CellSize(80, 25));
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: const FlarkMarkdown(markdown: '# Article\n\n**body**'),
      ),
    );
    await Future<void>.delayed(Duration.zero);
    tester.render();
    expect(tester.renderToString(), contains('Article'));
    expect(tester.renderToString(), contains('body'));
    final semantics = tester.semantics().single(
      role: SemanticRole.textArea,
      label: 'Markdown',
    );
    expect(semantics.value, contains('body'));
    expect(semantics.actions, isEmpty);
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: const FlarkMarkdown(markdown: '# Changed'),
      ),
    );
    tester.render();
    expect(tester.renderToString(), contains('Changed'));
    tester.dispose();
  });
}
