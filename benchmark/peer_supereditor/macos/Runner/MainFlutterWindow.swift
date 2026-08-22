import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  // Instance initialization happens while the NIB constructs the first native
  // window, before Flutter engine startup and Dart fixture construction.
  private let processBootstrapUptimeMicros = UInt64(
    ProcessInfo.processInfo.systemUptime * 1_000_000
  )
  private var typingTimer: DispatchSourceTimer?
  private var savedPasteboardItems: [NSPasteboardItem]?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    self.contentViewController = flutterViewController
    self.setContentSize(NSSize(width: 680, height: 680))
    self.center()

    RegisterGeneratedPlugins(registry: flutterViewController)
    installBenchmarkInputChannel(on: flutterViewController)

    super.awakeFromNib()
  }

  private func installBenchmarkInputChannel(on controller: FlutterViewController) {
    let channel = FlutterMethodChannel(
      name: "dev.flark/competitor_input",
      binaryMessenger: controller.engine.binaryMessenger
    )
    channel.setMethodCallHandler { [weak self, weak controller] call, result in
      guard let self, let controller else {
        result(FlutterError(code: "window-gone", message: nil, details: nil))
        return
      }

      switch call.method {
      case "bootstrap":
        result([
          "processBootstrapUptimeMicros": self.processBootstrapUptimeMicros,
          "nativeNowUptimeMicros": UInt64(ProcessInfo.processInfo.systemUptime * 1_000_000),
          "processId": ProcessInfo.processInfo.processIdentifier,
        ])
      case "activate":
        NSApp.activate(ignoringOtherApps: true)
        self.makeKeyAndOrderFront(nil)
        // Once Flutter establishes its hidden NSTextInputClient, replacing it
        // with the outer Flutter view breaks AppKit edit actions. Seed a
        // responder only for the empty-window case and otherwise preserve the
        // active text-input client selected by Flutter.
        if self.firstResponder == nil {
          self.makeFirstResponder(controller.view)
        }
        result(true)
      case "text":
        guard
          let arguments = call.arguments as? [String: Any],
          let text = arguments["text"] as? String,
          !text.isEmpty
        else {
          result(FlutterError(code: "bad-text", message: nil, details: call.arguments))
          return
        }
        result(self.postKeyEvent(
          text: text,
          charactersIgnoringModifiers: text,
          keyCode: self.keyCode(for: text),
          modifiers: []
        ))
      case "backspace":
        result(self.postKeyEvent(
          text: "\u{7f}",
          charactersIgnoringModifiers: "\u{7f}",
          keyCode: 51,
          modifiers: []
        ))
      case "paste":
        guard
          let arguments = call.arguments as? [String: Any],
          let text = arguments["text"] as? String
        else {
          result(FlutterError(code: "bad-paste", message: nil, details: call.arguments))
          return
        }
        self.dispatchPaste(
          text: text,
          preferTextInputFallback: arguments["preferTextInputFallback"] as? Bool ?? false,
          result: result
        )
      case "pasteFallback":
        guard
          let arguments = call.arguments as? [String: Any],
          let text = arguments["text"] as? String
        else {
          result(FlutterError(code: "bad-paste-fallback", message: nil, details: call.arguments))
          return
        }
        self.dispatchTextInputPaste(text: text, result: result)
      case "restorePasteboard":
        self.restorePasteboard()
        result(nil)
      case "scheduleText":
        guard
          let arguments = call.arguments as? [String: Any],
          let text = arguments["text"] as? String,
          let cadenceMicros = arguments["cadenceMicros"] as? Int,
          cadenceMicros > 0
        else {
          result(FlutterError(code: "bad-schedule", message: nil, details: call.arguments))
          return
        }
        result(self.scheduleTextEvents(text: text, cadenceMicros: cadenceMicros))
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  private func keyCode(for text: String) -> UInt16 {
    return text == "x" ? 7 : 0
  }

  private func dispatchPaste(
    text: String,
    preferTextInputFallback: Bool,
    result: @escaping FlutterResult
  ) {
    let pasteboard = NSPasteboard.general
    restorePasteboard()
    savedPasteboardItems = Self.copyPasteboardItems(pasteboard.pasteboardItems ?? [])
    pasteboard.clearContents()
    guard pasteboard.setString(text, forType: .string) else {
      restorePasteboard()
      result(FlutterError(code: "pasteboard-write", message: nil, details: nil))
      return
    }

    DispatchQueue.main.async { [weak self] in
      guard let self else {
        result(FlutterError(code: "window-gone", message: nil, details: nil))
        return
      }
      NSApp.activate(ignoringOtherApps: true)
      self.makeKeyAndOrderFront(nil)

      // Capture the responder and NSTextInputClient selected by Flutter. Do
      // not replace it with FlutterView/FlutterViewWrapper: the hidden client
      // is the native platform text-input boundary used by a physical paste.
      let activeResponder = self.firstResponder
      let activeTextInput = activeResponder as? NSTextInputClient
      let responderClass = activeResponder.map {
        String(describing: type(of: $0))
      } ?? "nil"
      let dispatchUptime = ProcessInfo.processInfo.systemUptime

      // Prefer AppKit's ordinary responder action. Flutter's active text input
      // client may not advertise paste:, so use NSTextInputClient.insertText
      // as the physical-equivalent fallback. Both routes stay above the Dart
      // editor model and enter through Flutter's native text-input plugin.
      let actionSent = preferTextInputFallback ? false : NSApp.sendAction(
          #selector(NSText.paste(_:)),
          to: activeResponder,
          from: self
        )
      var textInputFallback = false
      if !actionSent, let activeTextInput {
        activeTextInput.insertText(
          text,
          replacementRange: NSRange(location: NSNotFound, length: 0)
        )
        textInputFallback = true
      }
      let platformRouteInvoked = actionSent || textInputFallback
      NSLog(
        "flark_peer_supereditor paste responder=%@ actionSent=%@ textInputFallback=%@",
        responderClass,
        actionSent ? "true" : "false",
        textInputFallback ? "true" : "false"
      )
      result([
        "postedUptimeMicros": UInt64(dispatchUptime * 1_000_000),
        "eventPath": actionSent
          ? "NSApplication.sendAction-paste-to-active-responder"
          : "NSTextInputClient.insertText-to-active-Flutter-text-input",
        "firstResponderClass": responderClass,
        "activeTextInputClient": activeTextInput != nil,
        "pasteActionSent": actionSent,
        "textInputFallback": textInputFallback,
        "preferredTextInputFallback": preferTextInputFallback,
        "platformRouteInvoked": platformRouteInvoked,
        "pasteboardType": NSPasteboard.PasteboardType.string.rawValue,
      ])
    }
  }

  private func dispatchTextInputPaste(
    text: String,
    result: @escaping FlutterResult
  ) {
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        result(FlutterError(code: "window-gone", message: nil, details: nil))
        return
      }
      NSApp.activate(ignoringOtherApps: true)
      self.makeKeyAndOrderFront(nil)
      let activeResponder = self.firstResponder
      let activeTextInput = activeResponder as? NSTextInputClient
      let responderClass = activeResponder.map {
        String(describing: type(of: $0))
      } ?? "nil"
      let dispatchUptime = ProcessInfo.processInfo.systemUptime
      if let activeTextInput {
        activeTextInput.insertText(
          text,
          replacementRange: NSRange(location: NSNotFound, length: 0)
        )
      }
      result([
        "postedUptimeMicros": UInt64(dispatchUptime * 1_000_000),
        "eventPath": "NSTextInputClient.insertText-after-unobserved-paste-action",
        "firstResponderClass": responderClass,
        "activeTextInputClient": activeTextInput != nil,
        "textInputFallback": activeTextInput != nil,
        "platformRouteInvoked": activeTextInput != nil,
      ])
    }
  }

  private func restorePasteboard() {
    guard let savedPasteboardItems else { return }
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    if !savedPasteboardItems.isEmpty {
      pasteboard.writeObjects(savedPasteboardItems)
    }
    self.savedPasteboardItems = nil
  }

  private static func copyPasteboardItems(
    _ items: [NSPasteboardItem]
  ) -> [NSPasteboardItem] {
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

  private func postKeyEvent(
    text: String,
    charactersIgnoringModifiers: String,
    keyCode: UInt16,
    modifiers: NSEvent.ModifierFlags,
    ensureFlutterViewFirstResponder: Bool = true,
    synthesizeModifierTransitions: Bool = false
  ) -> [String: Any] {
    NSApp.activate(ignoringOtherApps: true)
    makeKeyAndOrderFront(nil)
    if ensureFlutterViewFirstResponder,
       firstResponder == nil,
       let flutterView = contentViewController?.view {
      makeFirstResponder(flutterView)
    }
    let firstResponderClass = firstResponder.map {
      String(describing: type(of: $0))
    } ?? "nil"

    let timestamp = ProcessInfo.processInfo.systemUptime
    if synthesizeModifierTransitions, modifiers.contains(.command),
       let commandDown = NSEvent.keyEvent(
         with: .flagsChanged,
         location: .zero,
         modifierFlags: [.command],
         timestamp: timestamp,
         windowNumber: self.windowNumber,
         context: nil,
         characters: "",
         charactersIgnoringModifiers: "",
         isARepeat: false,
         keyCode: 55
       ) {
      NSApp.postEvent(commandDown, atStart: false)
    }
    let makeEvent: (NSEvent.EventType) -> NSEvent? = { eventType in
      NSEvent.keyEvent(
        with: eventType,
        location: .zero,
        modifierFlags: modifiers,
        timestamp: timestamp,
        windowNumber: self.windowNumber,
        context: nil,
        characters: text,
        charactersIgnoringModifiers: charactersIgnoringModifiers,
        isARepeat: false,
        keyCode: keyCode
      )
    }

    if let keyDown = makeEvent(.keyDown) {
      NSApp.postEvent(keyDown, atStart: false)
    }
    if let keyUp = makeEvent(.keyUp) {
      NSApp.postEvent(keyUp, atStart: false)
    }
    if synthesizeModifierTransitions, modifiers.contains(.command),
       let commandUp = NSEvent.keyEvent(
         with: .flagsChanged,
         location: .zero,
         modifierFlags: [],
         timestamp: timestamp,
         windowNumber: self.windowNumber,
         context: nil,
         characters: "",
         charactersIgnoringModifiers: "",
         isARepeat: false,
         keyCode: 55
       ) {
      NSApp.postEvent(commandUp, atStart: false)
    }

    return [
      "postedUptimeMicros": UInt64(timestamp * 1_000_000),
      "eventPath": "NSApplication.postEvent-to-Flutter-macOS-text-input",
      "firstResponderClass": firstResponderClass,
      "keyCode": Int(keyCode),
      "modifierFlags": modifiers.rawValue,
      "modifierTransitionsSynthesized": synthesizeModifierTransitions,
    ]
  }

  private func scheduleTextEvents(text: String, cadenceMicros: Int) -> [String: Any] {
    typingTimer?.cancel()
    NSApp.activate(ignoringOtherApps: true)
    makeKeyAndOrderFront(nil)
    if firstResponder == nil, let flutterView = contentViewController?.view {
      makeFirstResponder(flutterView)
    }

    let characters = text.map(String.init)
    let initialDelayMicros = 200_000
    let startUptimeMicros = UInt64(
      ProcessInfo.processInfo.systemUptime * 1_000_000
    ) + UInt64(initialDelayMicros)
    let windowNumber = self.windowNumber
    let timer = DispatchSource.makeTimerSource(
      queue: DispatchQueue(label: "dev.flark.competitor-typing", qos: .userInteractive)
    )
    var index = 0
    timer.schedule(
      deadline: .now() + .microseconds(initialDelayMicros),
      repeating: .microseconds(cadenceMicros),
      leeway: .microseconds(100)
    )
    timer.setEventHandler { [weak timer] in
      guard index < characters.count else {
        timer?.cancel()
        return
      }
      let character = characters[index]
      let timestamp = Double(startUptimeMicros + UInt64(index * cadenceMicros)) / 1_000_000
      index += 1
      let keyCode: UInt16 = character == "x" ? 7 : 0
      for eventType in [NSEvent.EventType.keyDown, .keyUp] {
        guard let event = NSEvent.keyEvent(
          with: eventType,
          location: .zero,
          modifierFlags: [],
          timestamp: timestamp,
          windowNumber: windowNumber,
          context: nil,
          characters: character,
          charactersIgnoringModifiers: character,
          isARepeat: false,
          keyCode: keyCode
        ) else {
          continue
        }
        // The source queue is independent from Flutter's merged UI/platform
        // thread, matching physical events that continue while a long frame
        // is in progress.
        NSApp.postEvent(event, atStart: false)
      }
    }
    typingTimer = timer
    timer.resume()

    return [
      "scheduledStartUptimeMicros": startUptimeMicros,
      "cadenceMicros": cadenceMicros,
      "eventCount": characters.count,
      "eventPath": "background-timer-to-NSApplication.postEvent-to-Flutter-macOS-text-input",
    ]
  }
}
