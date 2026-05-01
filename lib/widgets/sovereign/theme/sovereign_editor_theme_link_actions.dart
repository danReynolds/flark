import 'package:flutter/material.dart';

@immutable
class SovereignLinkActionsTheme {
  // Overlay layout metrics.
  final double overlayViewportHorizontalPadding;
  final double overlayMinWidth;
  final double overlayMaxLinkWidth;
  final double overlayMaxInlineImageWidth;
  final double overlayMaxStandaloneImageWidth;
  final double estimatedLinkWidth;
  final double estimatedInlineImageWidth;
  final double estimatedStandaloneImageWidth;
  final double estimatedLinkHeight;
  final double estimatedInlineImageHeight;
  final double estimatedStandaloneImageHeight;
  final double editorEdgePadding;
  final double overlayAboveCaretOffset;
  final double overlayBelowCaretOffset;
  final double overlayVerticalPadding;

  final EdgeInsets padding;
  final EdgeInsets margin;
  final EdgeInsets actionPadding;
  final Color backgroundColor;
  final Color borderColor;
  final Color? actionBackgroundColor;
  final Color? actionBorderColor;
  final TextStyle textStyle;
  final Color iconColor;
  final double borderRadius;
  final double actionBorderRadius;
  final double actionGap;
  final double elevation;
  final Color? imagePreviewBackgroundColor;
  final Color? imagePreviewBorderColor;
  final Color? imagePreviewLoadingBackgroundColor;
  final Color? imagePreviewErrorBackgroundColor;
  final Color? imageCaptionBackgroundColor;
  final Color? imageCaptionBorderColor;
  final EdgeInsets imageCaptionPadding;
  final TextStyle? imageCaptionTextStyle;
  final TextStyle? imageUrlTextStyle;

  const SovereignLinkActionsTheme({
    this.overlayViewportHorizontalPadding = 16,
    this.overlayMinWidth = 120,
    this.overlayMaxLinkWidth = 360,
    this.overlayMaxInlineImageWidth = 420,
    this.overlayMaxStandaloneImageWidth = 560,
    this.estimatedLinkWidth = 250,
    this.estimatedInlineImageWidth = 320,
    this.estimatedStandaloneImageWidth = 420,
    this.estimatedLinkHeight = 42,
    this.estimatedInlineImageHeight = 248,
    this.estimatedStandaloneImageHeight = 336,
    this.editorEdgePadding = 8,
    this.overlayAboveCaretOffset = 34,
    this.overlayBelowCaretOffset = 4,
    this.overlayVerticalPadding = 4,
    this.padding = const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
    this.margin = const EdgeInsets.only(top: 6, right: 6),
    this.actionPadding = const EdgeInsets.symmetric(
      horizontal: 10,
      vertical: 6,
    ),
    this.backgroundColor = const Color(0xF020252C),
    this.borderColor = const Color(0x33FFFFFF),
    this.actionBackgroundColor = const Color(0x14FFFFFF),
    this.actionBorderColor = const Color(0x14FFFFFF),
    this.textStyle = const TextStyle(
      color: Colors.white,
      fontSize: 11,
      fontWeight: FontWeight.w600,
    ),
    this.iconColor = const Color(0xD9FFFFFF),
    this.borderRadius = 10,
    this.actionBorderRadius = 8,
    this.actionGap = 6,
    this.elevation = 4,
    this.imagePreviewBackgroundColor,
    this.imagePreviewBorderColor,
    this.imagePreviewLoadingBackgroundColor,
    this.imagePreviewErrorBackgroundColor,
    this.imageCaptionBackgroundColor,
    this.imageCaptionBorderColor,
    this.imageCaptionPadding = const EdgeInsets.fromLTRB(10, 8, 10, 8),
    this.imageCaptionTextStyle,
    this.imageUrlTextStyle,
  });

  SovereignLinkActionsTheme copyWith({
    double? overlayViewportHorizontalPadding,
    double? overlayMinWidth,
    double? overlayMaxLinkWidth,
    double? overlayMaxInlineImageWidth,
    double? overlayMaxStandaloneImageWidth,
    double? estimatedLinkWidth,
    double? estimatedInlineImageWidth,
    double? estimatedStandaloneImageWidth,
    double? estimatedLinkHeight,
    double? estimatedInlineImageHeight,
    double? estimatedStandaloneImageHeight,
    double? editorEdgePadding,
    double? overlayAboveCaretOffset,
    double? overlayBelowCaretOffset,
    double? overlayVerticalPadding,
    EdgeInsets? padding,
    EdgeInsets? margin,
    EdgeInsets? actionPadding,
    Color? backgroundColor,
    Color? borderColor,
    Color? actionBackgroundColor,
    Color? actionBorderColor,
    TextStyle? textStyle,
    Color? iconColor,
    double? borderRadius,
    double? actionBorderRadius,
    double? actionGap,
    double? elevation,
    Color? imagePreviewBackgroundColor,
    Color? imagePreviewBorderColor,
    Color? imagePreviewLoadingBackgroundColor,
    Color? imagePreviewErrorBackgroundColor,
    Color? imageCaptionBackgroundColor,
    Color? imageCaptionBorderColor,
    EdgeInsets? imageCaptionPadding,
    TextStyle? imageCaptionTextStyle,
    TextStyle? imageUrlTextStyle,
  }) {
    return SovereignLinkActionsTheme(
      overlayViewportHorizontalPadding: overlayViewportHorizontalPadding ??
          this.overlayViewportHorizontalPadding,
      overlayMinWidth: overlayMinWidth ?? this.overlayMinWidth,
      overlayMaxLinkWidth: overlayMaxLinkWidth ?? this.overlayMaxLinkWidth,
      overlayMaxInlineImageWidth:
          overlayMaxInlineImageWidth ?? this.overlayMaxInlineImageWidth,
      overlayMaxStandaloneImageWidth:
          overlayMaxStandaloneImageWidth ?? this.overlayMaxStandaloneImageWidth,
      estimatedLinkWidth: estimatedLinkWidth ?? this.estimatedLinkWidth,
      estimatedInlineImageWidth:
          estimatedInlineImageWidth ?? this.estimatedInlineImageWidth,
      estimatedStandaloneImageWidth:
          estimatedStandaloneImageWidth ?? this.estimatedStandaloneImageWidth,
      estimatedLinkHeight: estimatedLinkHeight ?? this.estimatedLinkHeight,
      estimatedInlineImageHeight:
          estimatedInlineImageHeight ?? this.estimatedInlineImageHeight,
      estimatedStandaloneImageHeight:
          estimatedStandaloneImageHeight ?? this.estimatedStandaloneImageHeight,
      editorEdgePadding: editorEdgePadding ?? this.editorEdgePadding,
      overlayAboveCaretOffset:
          overlayAboveCaretOffset ?? this.overlayAboveCaretOffset,
      overlayBelowCaretOffset:
          overlayBelowCaretOffset ?? this.overlayBelowCaretOffset,
      overlayVerticalPadding:
          overlayVerticalPadding ?? this.overlayVerticalPadding,
      padding: padding ?? this.padding,
      margin: margin ?? this.margin,
      actionPadding: actionPadding ?? this.actionPadding,
      backgroundColor: backgroundColor ?? this.backgroundColor,
      borderColor: borderColor ?? this.borderColor,
      actionBackgroundColor:
          actionBackgroundColor ?? this.actionBackgroundColor,
      actionBorderColor: actionBorderColor ?? this.actionBorderColor,
      textStyle: textStyle ?? this.textStyle,
      iconColor: iconColor ?? this.iconColor,
      borderRadius: borderRadius ?? this.borderRadius,
      actionBorderRadius: actionBorderRadius ?? this.actionBorderRadius,
      actionGap: actionGap ?? this.actionGap,
      elevation: elevation ?? this.elevation,
      imagePreviewBackgroundColor:
          imagePreviewBackgroundColor ?? this.imagePreviewBackgroundColor,
      imagePreviewBorderColor:
          imagePreviewBorderColor ?? this.imagePreviewBorderColor,
      imagePreviewLoadingBackgroundColor: imagePreviewLoadingBackgroundColor ??
          this.imagePreviewLoadingBackgroundColor,
      imagePreviewErrorBackgroundColor: imagePreviewErrorBackgroundColor ??
          this.imagePreviewErrorBackgroundColor,
      imageCaptionBackgroundColor:
          imageCaptionBackgroundColor ?? this.imageCaptionBackgroundColor,
      imageCaptionBorderColor:
          imageCaptionBorderColor ?? this.imageCaptionBorderColor,
      imageCaptionPadding: imageCaptionPadding ?? this.imageCaptionPadding,
      imageCaptionTextStyle:
          imageCaptionTextStyle ?? this.imageCaptionTextStyle,
      imageUrlTextStyle: imageUrlTextStyle ?? this.imageUrlTextStyle,
    );
  }
}

@immutable
class SovereignLinkEditDialogTheme {
  final Color barrierColor;
  final Color backgroundColor;
  final Color borderColor;
  final TextStyle titleStyle;
  final TextStyle fieldTextStyle;
  final TextStyle fieldLabelStyle;
  final Color fieldFillColor;
  final Color fieldBorderColor;
  final Color fieldFocusedBorderColor;
  final Color cancelForegroundColor;
  final Color saveBackgroundColor;
  final Color saveForegroundColor;
  final double borderRadius;
  final double width;

  const SovereignLinkEditDialogTheme({
    this.barrierColor = const Color(0x66000000),
    this.backgroundColor = const Color(0xFF1E232B),
    this.borderColor = const Color(0x33FFFFFF),
    this.titleStyle = const TextStyle(
      color: Colors.white,
      fontSize: 16,
      fontWeight: FontWeight.w700,
    ),
    this.fieldTextStyle = const TextStyle(
      color: Colors.white,
      fontSize: 13.5,
      fontWeight: FontWeight.w500,
    ),
    this.fieldLabelStyle = const TextStyle(
      color: Color(0xB3FFFFFF),
      fontSize: 12,
      fontWeight: FontWeight.w500,
    ),
    this.fieldFillColor = const Color(0x15262D36),
    this.fieldBorderColor = const Color(0x33FFFFFF),
    this.fieldFocusedBorderColor = const Color(0x88FFFFFF),
    this.cancelForegroundColor = const Color(0xCCFFFFFF),
    this.saveBackgroundColor = const Color(0xFFE6C27A),
    this.saveForegroundColor = const Color(0xFF121417),
    this.borderRadius = 14,
    this.width = 420,
  });

  SovereignLinkEditDialogTheme copyWith({
    Color? barrierColor,
    Color? backgroundColor,
    Color? borderColor,
    TextStyle? titleStyle,
    TextStyle? fieldTextStyle,
    TextStyle? fieldLabelStyle,
    Color? fieldFillColor,
    Color? fieldBorderColor,
    Color? fieldFocusedBorderColor,
    Color? cancelForegroundColor,
    Color? saveBackgroundColor,
    Color? saveForegroundColor,
    double? borderRadius,
    double? width,
  }) {
    return SovereignLinkEditDialogTheme(
      barrierColor: barrierColor ?? this.barrierColor,
      backgroundColor: backgroundColor ?? this.backgroundColor,
      borderColor: borderColor ?? this.borderColor,
      titleStyle: titleStyle ?? this.titleStyle,
      fieldTextStyle: fieldTextStyle ?? this.fieldTextStyle,
      fieldLabelStyle: fieldLabelStyle ?? this.fieldLabelStyle,
      fieldFillColor: fieldFillColor ?? this.fieldFillColor,
      fieldBorderColor: fieldBorderColor ?? this.fieldBorderColor,
      fieldFocusedBorderColor:
          fieldFocusedBorderColor ?? this.fieldFocusedBorderColor,
      cancelForegroundColor:
          cancelForegroundColor ?? this.cancelForegroundColor,
      saveBackgroundColor: saveBackgroundColor ?? this.saveBackgroundColor,
      saveForegroundColor: saveForegroundColor ?? this.saveForegroundColor,
      borderRadius: borderRadius ?? this.borderRadius,
      width: width ?? this.width,
    );
  }
}
