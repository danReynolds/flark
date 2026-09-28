/// The live-rendering byte limit a session uses unless the app sets one:
/// 32 KiB on desktop and in desktop browsers, 16 KiB on phones and tablets
/// until a receipt on one raises it.
library;

export 'platform_limits_io.dart'
    if (dart.library.js_interop) 'platform_limits_web.dart';
