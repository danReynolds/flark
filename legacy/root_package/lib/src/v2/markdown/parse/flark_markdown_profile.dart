enum FlarkMarkdownProfile { commonMarkCore, commonMarkGfm }

extension FlarkMarkdownProfileWire on FlarkMarkdownProfile {
  String get wireName {
    return switch (this) {
      FlarkMarkdownProfile.commonMarkCore => 'commonMarkCore',
      FlarkMarkdownProfile.commonMarkGfm => 'commonMarkGfm',
    };
  }

  static FlarkMarkdownProfile fromWireName(String value) {
    return switch (value) {
      'commonMarkCore' => FlarkMarkdownProfile.commonMarkCore,
      'commonMarkGfm' => FlarkMarkdownProfile.commonMarkGfm,
      _ => FlarkMarkdownProfile.commonMarkCore,
    };
  }
}
