import '../../core/core.dart';
import 'flark_markdown_block_commands.dart';
import 'flark_markdown_image_commands.dart';
import 'flark_markdown_inline_commands.dart';
import 'flark_markdown_input_commands.dart';
import 'flark_markdown_link_commands.dart';
import 'flark_markdown_table_commands.dart';

abstract final class FlarkMarkdownEditingExtensions {
  static FlarkExtensionSet standard() {
    return FlarkExtensionSet(const [
      FlarkCoreEditingExtension(),
      FlarkMarkdownInlineEditingExtension(),
      FlarkMarkdownBlockEditingExtension(),
      FlarkMarkdownInputEditingExtension(),
      FlarkMarkdownLinkEditingExtension(),
      FlarkMarkdownImageEditingExtension(),
      FlarkMarkdownTableEditingExtension(),
    ]);
  }
}
