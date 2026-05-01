import 'package:flutter/material.dart';

import 'package:sovereign_editor/theme/sovereign_markdown_theme.dart';
import 'sovereign_editor_theme_link_actions.dart';
export 'sovereign_editor_theme_link_actions.dart';

@immutable
class SovereignInlineTextTheme {
  final TextStyle? bold;
  final TextStyle? italic;
  final TextStyle? inlineCode;
  final TextStyle? link;
  final TextStyle? image;

  const SovereignInlineTextTheme({
    this.bold,
    this.italic,
    this.inlineCode,
    this.link,
    this.image,
  });

  SovereignInlineTextTheme copyWith({
    TextStyle? bold,
    TextStyle? italic,
    TextStyle? inlineCode,
    TextStyle? link,
    TextStyle? image,
  }) {
    return SovereignInlineTextTheme(
      bold: bold ?? this.bold,
      italic: italic ?? this.italic,
      inlineCode: inlineCode ?? this.inlineCode,
      link: link ?? this.link,
      image: image ?? this.image,
    );
  }
}

@immutable
class SovereignHeadingsTheme {
  final TextStyle? h1;
  final TextStyle? h2;
  final TextStyle? h3;
  final TextStyle? h4;
  final TextStyle? h5;
  final TextStyle? h6;

  const SovereignHeadingsTheme({
    this.h1,
    this.h2,
    this.h3,
    this.h4,
    this.h5,
    this.h6,
  });

  SovereignHeadingsTheme copyWith({
    TextStyle? h1,
    TextStyle? h2,
    TextStyle? h3,
    TextStyle? h4,
    TextStyle? h5,
    TextStyle? h6,
  }) {
    return SovereignHeadingsTheme(
      h1: h1 ?? this.h1,
      h2: h2 ?? this.h2,
      h3: h3 ?? this.h3,
      h4: h4 ?? this.h4,
      h5: h5 ?? this.h5,
      h6: h6 ?? this.h6,
    );
  }

  TextStyle? styleForLevel(int level) {
    switch (level.clamp(1, 6)) {
      case 1:
        return h1;
      case 2:
        return h2;
      case 3:
        return h3;
      case 4:
        return h4;
      case 5:
        return h5;
      case 6:
      default:
        return h6;
    }
  }
}

@immutable
class SovereignBlockquoteTheme {
  final Color? railColor;
  final double railWidth;
  final double railInset;
  final Radius railRadius;

  const SovereignBlockquoteTheme({
    this.railColor,
    this.railWidth = 4.0,
    this.railInset = 0.0,
    this.railRadius = const Radius.circular(2.0),
  });

  SovereignBlockquoteTheme copyWith({
    Color? railColor,
    double? railWidth,
    double? railInset,
    Radius? railRadius,
  }) {
    return SovereignBlockquoteTheme(
      railColor: railColor ?? this.railColor,
      railWidth: railWidth ?? this.railWidth,
      railInset: railInset ?? this.railInset,
      railRadius: railRadius ?? this.railRadius,
    );
  }
}

@immutable
class SovereignFenceLanguagePickerTheme {
  final EdgeInsets padding;
  final EdgeInsets margin;
  final Color backgroundColor;
  final Color borderColor;
  final TextStyle textStyle;
  final TextStyle menuTextStyle;
  final Color menuBackgroundColor;
  final Color iconColor;
  final double iconSize;
  final double iconGap;
  final double borderRadius;
  final double maxWidth;
  final double verticalOffset;
  final double height;
  final double elevation;

  const SovereignFenceLanguagePickerTheme({
    this.padding = const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
    this.margin = const EdgeInsets.only(top: 2, right: 6),
    this.backgroundColor = const Color(0xEE20252C),
    this.borderColor = const Color(0x4DFFFFFF),
    this.textStyle = const TextStyle(
      color: Colors.white,
      fontSize: 11,
      fontWeight: FontWeight.w600,
      height: 1.0,
    ),
    this.menuTextStyle = const TextStyle(
      color: Colors.white,
      fontSize: 13,
      fontWeight: FontWeight.w500,
    ),
    this.menuBackgroundColor = const Color(0xFF1E2229),
    this.iconColor = const Color(0xCCFFFFFF),
    this.iconSize = 14,
    this.iconGap = 2,
    this.borderRadius = 8,
    this.maxWidth = 118,
    this.verticalOffset = 2,
    this.height = 20,
    this.elevation = 4,
  });

  SovereignFenceLanguagePickerTheme copyWith({
    EdgeInsets? padding,
    EdgeInsets? margin,
    Color? backgroundColor,
    Color? borderColor,
    TextStyle? textStyle,
    TextStyle? menuTextStyle,
    Color? menuBackgroundColor,
    Color? iconColor,
    double? iconSize,
    double? iconGap,
    double? borderRadius,
    double? maxWidth,
    double? verticalOffset,
    double? height,
    double? elevation,
  }) {
    return SovereignFenceLanguagePickerTheme(
      padding: padding ?? this.padding,
      margin: margin ?? this.margin,
      backgroundColor: backgroundColor ?? this.backgroundColor,
      borderColor: borderColor ?? this.borderColor,
      textStyle: textStyle ?? this.textStyle,
      menuTextStyle: menuTextStyle ?? this.menuTextStyle,
      menuBackgroundColor: menuBackgroundColor ?? this.menuBackgroundColor,
      iconColor: iconColor ?? this.iconColor,
      iconSize: iconSize ?? this.iconSize,
      iconGap: iconGap ?? this.iconGap,
      borderRadius: borderRadius ?? this.borderRadius,
      maxWidth: maxWidth ?? this.maxWidth,
      verticalOffset: verticalOffset ?? this.verticalOffset,
      height: height ?? this.height,
      elevation: elevation ?? this.elevation,
    );
  }
}

@immutable
class SovereignCodeBlockTheme {
  final Color? backgroundColor;
  // Positive values shrink the painted fence background inward.
  // Negative values let the background bleed outward, which can visually
  // create content inset while keeping editor text edge-aligned.
  final double backgroundHorizontalInset;
  final double backgroundVerticalInset;
  final SovereignFenceLanguagePickerTheme languagePicker;

  const SovereignCodeBlockTheme({
    this.backgroundColor,
    this.backgroundHorizontalInset = 2.0,
    this.backgroundVerticalInset = 1.0,
    this.languagePicker = const SovereignFenceLanguagePickerTheme(),
  });

  SovereignCodeBlockTheme copyWith({
    Color? backgroundColor,
    double? backgroundHorizontalInset,
    double? backgroundVerticalInset,
    SovereignFenceLanguagePickerTheme? languagePicker,
  }) {
    return SovereignCodeBlockTheme(
      backgroundColor: backgroundColor ?? this.backgroundColor,
      backgroundHorizontalInset:
          backgroundHorizontalInset ?? this.backgroundHorizontalInset,
      backgroundVerticalInset:
          backgroundVerticalInset ?? this.backgroundVerticalInset,
      languagePicker: languagePicker ?? this.languagePicker,
    );
  }
}

@immutable
class SovereignTaskCheckboxTheme {
  final bool useCustomOverlay;
  final double size;
  final double borderRadius;
  final double borderWidth;
  final double horizontalInset;
  final double verticalInset;
  final Color checkedFillColor;
  final Color checkedBorderColor;
  final Color uncheckedFillColor;
  final Color uncheckedBorderColor;
  final Color checkColor;
  final double checkIconSize;
  final int labelGapSpaces;
  final TextStyle? checked;
  final TextStyle? unchecked;

  const SovereignTaskCheckboxTheme({
    this.useCustomOverlay = true,
    this.size = 14,
    this.borderRadius = 4,
    this.borderWidth = 1.2,
    this.horizontalInset = 1.0,
    this.verticalInset = 0.0,
    this.checkedFillColor = const Color(0xFFE6C27A),
    this.checkedBorderColor = const Color(0xFFE6C27A),
    this.uncheckedFillColor = const Color(0x14000000),
    this.uncheckedBorderColor = const Color(0x66FFFFFF),
    this.checkColor = const Color(0xFF14181D),
    this.checkIconSize = 11,
    this.labelGapSpaces = 1,
    this.checked,
    this.unchecked,
  });

  SovereignTaskCheckboxTheme copyWith({
    bool? useCustomOverlay,
    double? size,
    double? borderRadius,
    double? borderWidth,
    double? horizontalInset,
    double? verticalInset,
    Color? checkedFillColor,
    Color? checkedBorderColor,
    Color? uncheckedFillColor,
    Color? uncheckedBorderColor,
    Color? checkColor,
    double? checkIconSize,
    int? labelGapSpaces,
    TextStyle? checked,
    TextStyle? unchecked,
  }) {
    return SovereignTaskCheckboxTheme(
      useCustomOverlay: useCustomOverlay ?? this.useCustomOverlay,
      size: size ?? this.size,
      borderRadius: borderRadius ?? this.borderRadius,
      borderWidth: borderWidth ?? this.borderWidth,
      horizontalInset: horizontalInset ?? this.horizontalInset,
      verticalInset: verticalInset ?? this.verticalInset,
      checkedFillColor: checkedFillColor ?? this.checkedFillColor,
      checkedBorderColor: checkedBorderColor ?? this.checkedBorderColor,
      uncheckedFillColor: uncheckedFillColor ?? this.uncheckedFillColor,
      uncheckedBorderColor: uncheckedBorderColor ?? this.uncheckedBorderColor,
      checkColor: checkColor ?? this.checkColor,
      checkIconSize: checkIconSize ?? this.checkIconSize,
      labelGapSpaces: labelGapSpaces ?? this.labelGapSpaces,
      checked: checked ?? this.checked,
      unchecked: unchecked ?? this.unchecked,
    );
  }
}

@immutable
class SovereignEditorThemeData {
  final SovereignMarkdownTheme? markdownTheme;
  final TextStyle? textStyle;
  final Color? cursorColor;
  final EdgeInsets editorContentPadding;
  final SovereignHeadingsTheme headings;
  final SovereignInlineTextTheme inlineText;
  final SovereignBlockquoteTheme blockquote;
  final SovereignCodeBlockTheme codeBlock;
  final SovereignTaskCheckboxTheme taskCheckbox;
  final SovereignLinkActionsTheme linkActions;
  final SovereignLinkEditDialogTheme linkEditDialog;

  const SovereignEditorThemeData({
    this.markdownTheme,
    this.textStyle,
    this.cursorColor,
    this.editorContentPadding = EdgeInsets.zero,
    this.headings = const SovereignHeadingsTheme(),
    this.inlineText = const SovereignInlineTextTheme(),
    this.blockquote = const SovereignBlockquoteTheme(),
    this.codeBlock = const SovereignCodeBlockTheme(),
    this.taskCheckbox = const SovereignTaskCheckboxTheme(),
    this.linkActions = const SovereignLinkActionsTheme(),
    this.linkEditDialog = const SovereignLinkEditDialogTheme(),
  });

  SovereignEditorThemeData copyWith({
    SovereignMarkdownTheme? markdownTheme,
    TextStyle? textStyle,
    Color? cursorColor,
    EdgeInsets? editorContentPadding,
    SovereignHeadingsTheme? headings,
    SovereignInlineTextTheme? inlineText,
    SovereignBlockquoteTheme? blockquote,
    SovereignCodeBlockTheme? codeBlock,
    SovereignTaskCheckboxTheme? taskCheckbox,
    SovereignLinkActionsTheme? linkActions,
    SovereignLinkEditDialogTheme? linkEditDialog,
    bool clearMarkdownTheme = false,
    bool clearTextStyle = false,
    bool clearCursorColor = false,
  }) {
    return SovereignEditorThemeData(
      markdownTheme:
          clearMarkdownTheme ? null : (markdownTheme ?? this.markdownTheme),
      textStyle: clearTextStyle ? null : (textStyle ?? this.textStyle),
      cursorColor: clearCursorColor ? null : (cursorColor ?? this.cursorColor),
      editorContentPadding: editorContentPadding ?? this.editorContentPadding,
      headings: headings ?? this.headings,
      inlineText: inlineText ?? this.inlineText,
      blockquote: blockquote ?? this.blockquote,
      codeBlock: codeBlock ?? this.codeBlock,
      taskCheckbox: taskCheckbox ?? this.taskCheckbox,
      linkActions: linkActions ?? this.linkActions,
      linkEditDialog: linkEditDialog ?? this.linkEditDialog,
    );
  }

  SovereignMarkdownTheme resolveMarkdownTheme(BuildContext context) {
    return markdownTheme ?? SovereignMarkdownTheme.of(context);
  }
}

class SovereignEditorThemeScope extends InheritedWidget {
  final SovereignEditorThemeData data;

  const SovereignEditorThemeScope({
    super.key,
    required this.data,
    required super.child,
  });

  static SovereignEditorThemeData? maybeOf(BuildContext context) {
    return context
        .dependOnInheritedWidgetOfExactType<SovereignEditorThemeScope>()
        ?.data;
  }

  static SovereignEditorThemeData of(BuildContext context) {
    return maybeOf(context) ?? const SovereignEditorThemeData();
  }

  @override
  bool updateShouldNotify(covariant SovereignEditorThemeScope oldWidget) {
    return oldWidget.data != data;
  }
}
