import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  private var latencyCriticalActivity: NSObjectProtocol?

  override func applicationDidFinishLaunching(_ notification: Notification) {
    // Frame receipts are wall-clock evidence: an occluded, napping, or
    // display-asleep session has no honest vsync, so the example fronts
    // itself and holds the app, system, and display awake for its lifetime.
    NSApp.activate(ignoringOtherApps: true)
    latencyCriticalActivity = ProcessInfo.processInfo.beginActivity(
      options: [
        .userInitiated,
        .latencyCritical,
        .idleDisplaySleepDisabled,
        .idleSystemSleepDisabled,
      ],
      reason: "flark editor frame evidence"
    )
    super.applicationDidFinishLaunching(notification)
  }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
