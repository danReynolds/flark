/// Live bytes for a browser, from its user agent and touch points: 16 KiB on
/// phones and tablets, which no receipt covers yet, and 32 KiB on desktop,
/// where a desktop Chrome receipt passed the frame gate at 32 KiB.
int browserLiveBytes(String userAgent, int maxTouchPoints) {
  final mobile =
      RegExp('Android|iPhone|iPad|iPod|Mobi').hasMatch(userAgent) ||
      // iPadOS Safari reports a Mac, and a Mac has no touch screen.
      (userAgent.contains('Macintosh') && maxTouchPoints > 1);
  return mobile ? 16 * 1024 : 32 * 1024;
}
