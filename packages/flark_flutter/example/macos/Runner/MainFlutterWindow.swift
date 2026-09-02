import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    var windowFrame = self.frame
    let environment = ProcessInfo.processInfo.environment
    if let widthText = environment["FLARK_CANARY_INITIAL_WINDOW_WIDTH"],
      let heightText = environment["FLARK_CANARY_INITIAL_WINDOW_HEIGHT"],
      let width = Int(widthText),
      let height = Int(heightText),
      width > 0,
      height > 0
    {
      windowFrame.size = NSSize(width: width, height: height)
    }
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)

    self.makeKeyAndOrderFront(nil)
    self.orderFrontRegardless()

    super.awakeFromNib()
  }
}
