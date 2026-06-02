import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    self.contentViewController = flutterViewController
    self.title = "Sovereign Markdown"
    self.minSize = NSSize(width: 1040, height: 720)
    if let screenFrame = NSScreen.main?.visibleFrame {
      let initialSize = NSSize(width: 1280, height: 840)
      let origin = NSPoint(
        x: screenFrame.midX - initialSize.width / 2,
        y: screenFrame.midY - initialSize.height / 2
      )
      self.setFrame(NSRect(origin: origin, size: initialSize), display: true)
    }

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
