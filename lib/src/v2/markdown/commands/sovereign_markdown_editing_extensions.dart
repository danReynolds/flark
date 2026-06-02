import '../../core/core.dart';
import 'sovereign_markdown_block_commands.dart';
import 'sovereign_markdown_inline_commands.dart';
import 'sovereign_markdown_input_commands.dart';
import 'sovereign_markdown_link_commands.dart';
import 'sovereign_markdown_table_commands.dart';

abstract final class SovereignMarkdownEditingExtensions {
  static SovereignExtensionSet standard() {
    return SovereignExtensionSet(const [
      SovereignCoreEditingExtension(),
      SovereignMarkdownInlineEditingExtension(),
      SovereignMarkdownBlockEditingExtension(),
      SovereignMarkdownInputEditingExtension(),
      SovereignMarkdownLinkEditingExtension(),
      SovereignMarkdownTableEditingExtension(),
    ]);
  }
}
