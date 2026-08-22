import Cocoa
import Darwin
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  private weak var flutterController: FlutterViewController?
  private var savedPasteboardItems: [NSPasteboardItem]?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    flutterController = flutterViewController
    self.contentViewController = flutterViewController
    self.setContentSize(NSSize(width: 600, height: 600))
    self.contentMinSize = NSSize(width: 600, height: 600)
    self.contentMaxSize = NSSize(width: 600, height: 600)

    RegisterGeneratedPlugins(registry: flutterViewController)
    registerHarnessChannel(on: flutterViewController)

    super.awakeFromNib()
  }

  private func registerHarnessChannel(on controller: FlutterViewController) {
    let channel = FlutterMethodChannel(
      name: "dev.flark.peer_benchmark/harness",
      binaryMessenger: controller.engine.binaryMessenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "window_closed", message: "Runner window is gone", details: nil))
        return
      }
      switch call.method {
      case "activateWindow":
        self.activateForInput()
        result(nil)
      case "systemInfo":
        result(self.systemInfo())
      case "processMemory":
        result(self.processMemory())
      case "typeCharacter":
        guard
          let arguments = call.arguments as? [String: Any],
          let character = arguments["character"] as? String,
          character.count == 1,
          let keyCode = Self.keyCodes[character]
        else {
          result(FlutterError(code: "bad_character", message: "Expected one supported ASCII character", details: call.arguments))
          return
        }
        self.dispatchKey(
          characters: character,
          keyCode: keyCode,
          modifiers: [],
          result: result)
      case "backspace":
        self.dispatchKey(
          characters: "\u{8}",
          keyCode: 51,
          modifiers: [],
          result: result)
      case "pasteText":
        guard
          let arguments = call.arguments as? [String: Any],
          let text = arguments["text"] as? String
        else {
          result(FlutterError(code: "bad_paste", message: "Expected a text string", details: call.arguments))
          return
        }
        self.dispatchPaste(text: text, result: result)
      case "restoreClipboard":
        self.restoreClipboard()
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  private func activateForInput() {
    NSApp.activate(ignoringOtherApps: true)
    makeKeyAndOrderFront(nil)
    // Once Flutter opens a text-input connection its hidden
    // FlutterTextInputPlugin must remain first responder, especially for
    // key equivalents such as Command-V. Only seed the view responder before
    // that connection exists.
    if firstResponder == nil, let view = flutterController?.view {
      makeFirstResponder(view)
    }
  }

  private func dispatchKey(
    characters: String,
    keyCode: UInt16,
    modifiers: NSEvent.ModifierFlags,
    result: @escaping FlutterResult
  ) {
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        result(FlutterError(code: "window_closed", message: "Runner window is gone", details: nil))
        return
      }
      self.activateForInput()
      let dispatchUptime = ProcessInfo.processInfo.systemUptime
      let dispatchEpochMicros = Int64(Date().timeIntervalSince1970 * 1_000_000)
      guard
        let down = NSEvent.keyEvent(
          with: .keyDown,
          location: .zero,
          modifierFlags: modifiers,
          timestamp: dispatchUptime,
          windowNumber: self.windowNumber,
          context: nil,
          characters: characters,
          charactersIgnoringModifiers: characters,
          isARepeat: false,
          keyCode: keyCode),
        let up = NSEvent.keyEvent(
          with: .keyUp,
          location: .zero,
          modifierFlags: modifiers,
          timestamp: ProcessInfo.processInfo.systemUptime,
          windowNumber: self.windowNumber,
          context: nil,
          characters: characters,
          charactersIgnoringModifiers: characters,
          isARepeat: false,
          keyCode: keyCode)
      else {
        result(FlutterError(code: "event_creation_failed", message: "NSEvent.keyEvent returned nil", details: nil))
        return
      }
      let responderClass = self.firstResponder.map { String(describing: type(of: $0)) }
      let keyEquivalentHandled: Bool
      if modifiers.contains(.command), let responder = self.firstResponder as? NSView {
        keyEquivalentHandled = responder.performKeyEquivalent(with: down)
        if !keyEquivalentHandled {
          NSApp.sendEvent(down)
        }
      } else {
        keyEquivalentHandled = false
        NSApp.sendEvent(down)
      }
      if modifiers.contains(.command) {
        NSLog(
          "flark_peer_input command responder=%@ handled=%@",
          responderClass ?? "nil",
          keyEquivalentHandled ? "true" : "false")
      }
      NSApp.sendEvent(up)
      result([
        "dispatchUptimeMicros": Int64(dispatchUptime * 1_000_000),
        "dispatchEpochMicros": dispatchEpochMicros,
        "firstResponderClass": Self.codecValue(responderClass),
        "keyEquivalentHandled": keyEquivalentHandled,
      ])
    }
  }

  private func dispatchPaste(text: String, result: @escaping FlutterResult) {
    let pasteboard = NSPasteboard.general
    restoreClipboard()
    savedPasteboardItems = Self.copyPasteboardItems(pasteboard.pasteboardItems ?? [])
    pasteboard.clearContents()
    guard pasteboard.setString(text, forType: .string) else {
      result(FlutterError(code: "pasteboard_write_failed", message: "Could not write the paste payload", details: nil))
      return
    }
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        result(FlutterError(code: "window_closed", message: "Runner window is gone", details: nil))
        return
      }
      self.activateForInput()
      let dispatchUptime = ProcessInfo.processInfo.systemUptime
      let dispatchEpochMicros = Int64(Date().timeIntervalSince1970 * 1_000_000)
      let responderClass = self.firstResponder.map { String(describing: type(of: $0)) }
      let sent = NSApp.sendAction(#selector(NSText.paste(_:)), to: nil, from: self)
      var textInputFallback = false
      if !sent, let textInput = self.firstResponder as? NSTextInputClient {
        // FlutterTextInputPlugin does not advertise AppKit's paste: action,
        // even though it is the active NSTextInputClient. Deliver the exact
        // pasteboard string through that platform text-input client instead.
        textInput.insertText(
          text,
          replacementRange: NSRange(location: NSNotFound, length: 0))
        textInputFallback = true
      }
      NSLog(
        "flark_peer_input paste responder=%@ actionSent=%@ textInputFallback=%@",
        responderClass ?? "nil",
        sent ? "true" : "false",
        textInputFallback ? "true" : "false")
      result([
        "dispatchUptimeMicros": Int64(dispatchUptime * 1_000_000),
        "dispatchEpochMicros": dispatchEpochMicros,
        "firstResponderClass": Self.codecValue(responderClass),
        "pasteActionSent": sent,
        "textInputFallback": textInputFallback,
      ])
    }
  }

  private func restoreClipboard() {
    guard let savedItems = savedPasteboardItems else { return }
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    if !savedItems.isEmpty {
      pasteboard.writeObjects(savedItems)
    }
    savedPasteboardItems = nil
  }

  private static func copyPasteboardItems(_ items: [NSPasteboardItem]) -> [NSPasteboardItem] {
    items.map { source in
      let copy = NSPasteboardItem()
      for type in source.types {
        if let data = source.data(forType: type) {
          copy.setData(data, forType: type)
        }
      }
      return copy
    }
  }

  private func systemInfo() -> [String: Any] {
    let processInfo = ProcessInfo.processInfo
    let screen = self.screen ?? NSScreen.main
    let lowPowerModeEnabled: Any
    let displayRefreshRateHz: Any
    if #available(macOS 12.0, *) {
      lowPowerModeEnabled = processInfo.isLowPowerModeEnabled
      displayRefreshRateHz = Self.codecValue(screen?.maximumFramesPerSecond)
    } else {
      lowPowerModeEnabled = NSNull()
      displayRefreshRateHz = NSNull()
    }
    return [
      "processStartEpochMicros": Self.codecValue(Self.processStartEpochMicros()),
      "machineModel": Self.codecValue(Self.sysctlString("hw.model")),
      "cpuBrand": Self.codecValue(Self.sysctlString("machdep.cpu.brand_string")),
      "processorCount": processInfo.processorCount,
      "activeProcessorCount": processInfo.activeProcessorCount,
      "physicalMemoryBytes": processInfo.physicalMemory,
      "operatingSystemVersion": processInfo.operatingSystemVersionString,
      "lowPowerModeEnabled": lowPowerModeEnabled,
      "thermalState": Self.thermalStateName(processInfo.thermalState),
      "displayRefreshRateHz": displayRefreshRateHz,
      "devicePixelRatio": Self.codecValue(screen?.backingScaleFactor),
      "editorViewportLogicalWidth": 600,
      "editorViewportLogicalHeight": 600,
    ]
  }

  private func processMemory() -> [String: Any] {
    var taskInfo = mach_task_basic_info()
    var taskInfoCount = mach_msg_type_number_t(
      MemoryLayout<mach_task_basic_info>.stride / MemoryLayout<integer_t>.stride)
    let taskStatus = withUnsafeMutablePointer(to: &taskInfo) { taskInfoPointer in
      taskInfoPointer.withMemoryRebound(
        to: integer_t.self,
        capacity: Int(taskInfoCount)
      ) { reboundPointer in
        task_info(
          mach_task_self_,
          task_flavor_t(MACH_TASK_BASIC_INFO),
          reboundPointer,
          &taskInfoCount)
      }
    }
    var usage = rusage()
    let usageStatus = getrusage(RUSAGE_SELF, &usage)
    return [
      "currentResidentAvailable": taskStatus == KERN_SUCCESS,
      "residentBytes": Self.codecValue(
        taskStatus == KERN_SUCCESS ? taskInfo.resident_size : nil),
      "peakResidentAvailable": usageStatus == 0,
      "peakResidentBytes": Self.codecValue(
        usageStatus == 0 ? usage.ru_maxrss : nil),
      "physicalFootprintBytes": NSNull(),
      "physicalFootprintCaveat":
        "The minimal runner captures Mach resident size and getrusage peak RSS, not phys_footprint.",
    ]
  }

  private static func processStartEpochMicros() -> Int64? {
    var info = proc_bsdinfo()
    let expected = MemoryLayout<proc_bsdinfo>.stride
    let actual = proc_pidinfo(
      getpid(),
      PROC_PIDTBSDINFO,
      0,
      &info,
      Int32(expected))
    guard actual == expected else { return nil }
    return Int64(info.pbi_start_tvsec) * 1_000_000 + Int64(info.pbi_start_tvusec)
  }

  private static func sysctlString(_ name: String) -> String? {
    var size = 0
    guard sysctlbyname(name, nil, &size, nil, 0) == 0, size > 0 else { return nil }
    var value = [CChar](repeating: 0, count: size)
    guard sysctlbyname(name, &value, &size, nil, 0) == 0 else { return nil }
    return String(cString: value)
  }

  private static func thermalStateName(_ state: ProcessInfo.ThermalState) -> String {
    switch state {
    case .nominal: return "nominal"
    case .fair: return "fair"
    case .serious: return "serious"
    case .critical: return "critical"
    @unknown default: return "unknown"
    }
  }

  private static func codecValue<T>(_ value: T?) -> Any {
    value ?? NSNull()
  }

  private static let keyCodes: [String: UInt16] = [
    "a": 0, "b": 11, "c": 8, "d": 2, "e": 14, "f": 3,
    "g": 5, "h": 4, "i": 34, "j": 38, "k": 40, "l": 37,
    "m": 46, "n": 45, "o": 31, "p": 35, "q": 12, "r": 15,
    "s": 1, "t": 17, "u": 32, "v": 9, "w": 13, "x": 7,
    "y": 16, "z": 6,
    "0": 29, "1": 18, "2": 19, "3": 20, "4": 21,
    "5": 23, "6": 22, "7": 26, "8": 28, "9": 25,
  ]
}
