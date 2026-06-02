enum SovereignMarkdownProfile {
  commonMarkCore,
  commonMarkGfm,
}

extension SovereignMarkdownProfileWire on SovereignMarkdownProfile {
  String get wireName {
    return switch (this) {
      SovereignMarkdownProfile.commonMarkCore => 'commonMarkCore',
      SovereignMarkdownProfile.commonMarkGfm => 'commonMarkGfm',
    };
  }

  static SovereignMarkdownProfile fromWireName(String value) {
    return switch (value) {
      'commonMarkCore' => SovereignMarkdownProfile.commonMarkCore,
      'commonMarkGfm' => SovereignMarkdownProfile.commonMarkGfm,
      _ => SovereignMarkdownProfile.commonMarkCore,
    };
  }
}
