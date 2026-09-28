import 'dart:io' show Platform;

/// UTF-8 bytes rendered live by default on this platform: 16 KiB on phones,
/// 32 KiB on desktop.
int get flarkDefaultLiveBytes =>
    Platform.isIOS || Platform.isAndroid ? 16 * 1024 : 32 * 1024;
