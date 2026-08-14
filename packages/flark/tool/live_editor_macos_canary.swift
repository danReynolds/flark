import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

enum ActuatorFailure: Error, CustomStringConvertible {
  case message(String)

  var description: String {
    switch self {
    case .message(let value): value
    }
  }
}

func dictionary(_ value: Any?, _ label: String) throws -> [String: Any] {
  guard let result = value as? [String: Any] else {
    throw ActuatorFailure.message("missing object \(label)")
  }
  return result
}

func string(_ value: Any?, _ label: String) throws -> String {
  guard let result = value as? String else {
    throw ActuatorFailure.message("missing string \(label)")
  }
  return result
}

func integer(_ value: Any?, _ label: String) throws -> Int {
  guard let result = value as? Int else {
    throw ActuatorFailure.message("missing integer \(label)")
  }
  return result
}

func pause(milliseconds: Int) {
  usleep(useconds_t(max(0, milliseconds) * 1_000))
}

func readJSON(_ path: String) throws -> [String: Any] {
  let data = try Data(contentsOf: URL(fileURLWithPath: path))
  return try dictionary(JSONSerialization.jsonObject(with: data), "JSON document")
}

func waitUntil(
  _ label: String,
  timeoutSeconds: TimeInterval = 12,
  _ predicate: () throws -> Bool
) throws {
  let deadline = Date().addingTimeInterval(timeoutSeconds)
  while Date() < deadline {
    if try predicate() { return }
    pause(milliseconds: 20)
  }
  throw ActuatorFailure.message("\(label) timed out after \(timeoutSeconds)s")
}

func cgWindow(pid: pid_t) -> (number: Int, bounds: CGRect)? {
  guard let windows = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements],
    kCGNullWindowID
  ) as? [[String: Any]] else { return nil }
  for entry in windows {
    guard entry[kCGWindowOwnerPID as String] as? Int == Int(pid),
      let number = entry[kCGWindowNumber as String] as? Int,
      let boundsValue = entry[kCGWindowBounds as String] as? NSDictionary,
      let bounds = CGRect(dictionaryRepresentation: boundsValue)
    else { continue }
    return (number, bounds)
  }
  return nil
}

func setAXValue(
  _ element: AXUIElement,
  attribute: String,
  type: AXValueType,
  value: UnsafeRawPointer
) {
  if let wrapped = AXValueCreate(type, value) {
    AXUIElementSetAttributeValue(element, attribute as CFString, wrapped)
  }
}

func focusWindow(
  pid: pid_t,
  width: Int = 800,
  height: Int = 632
) throws -> (origin: CGPoint, size: CGSize, number: Int) {
  let application = AXUIElementCreateApplication(pid)
  var accessibleWindow: AXUIElement?
  try waitUntil("application window") {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(
      application,
      kAXWindowsAttribute as CFString,
      &value
    ) == .success,
      let windows = value as? [AXUIElement],
      let first = windows.first
    else { return false }
    accessibleWindow = first
    return true
  }
  guard let accessibleWindow else {
    throw ActuatorFailure.message("dogfood window was not accessible")
  }

  var requestedOrigin = CGPoint(x: 180, y: 100)
  var requestedSize = CGSize(width: width, height: height)
  setAXValue(
    accessibleWindow,
    attribute: kAXPositionAttribute,
    type: .cgPoint,
    value: &requestedOrigin
  )
  setAXValue(
    accessibleWindow,
    attribute: kAXSizeAttribute,
    type: .cgSize,
    value: &requestedSize
  )
  AXUIElementPerformAction(accessibleWindow, kAXRaiseAction as CFString)
  AXUIElementSetAttributeValue(
    accessibleWindow,
    kAXMainAttribute as CFString,
    kCFBooleanTrue
  )
  AXUIElementSetAttributeValue(
    accessibleWindow,
    kAXFocusedAttribute as CFString,
    kCFBooleanTrue
  )
  NSRunningApplication(processIdentifier: pid)?.activate(
    options: [.activateAllWindows]
  )
  pause(milliseconds: 250)

  if let window = cgWindow(pid: pid) {
    return (window.bounds.origin, window.bounds.size, window.number)
  }
  return (requestedOrigin, requestedSize, 0)
}

let eventSource = CGEventSource(stateID: .hidSystemState)

func click(_ location: CGPoint) {
  CGEvent(
    mouseEventSource: eventSource,
    mouseType: .mouseMoved,
    mouseCursorPosition: location,
    mouseButton: .left
  )?.post(tap: .cghidEventTap)
  pause(milliseconds: 20)
  let down = CGEvent(
    mouseEventSource: eventSource,
    mouseType: .leftMouseDown,
    mouseCursorPosition: location,
    mouseButton: .left
  )
  down?.setIntegerValueField(.mouseEventClickState, value: 1)
  down?.post(tap: .cghidEventTap)
  pause(milliseconds: 10)
  let up = CGEvent(
    mouseEventSource: eventSource,
    mouseType: .leftMouseUp,
    mouseCursorPosition: location,
    mouseButton: .left
  )
  up?.setIntegerValueField(.mouseEventClickState, value: 1)
  up?.post(tap: .cghidEventTap)
  pause(milliseconds: 100)
}

func drag(from start: CGPoint, to end: CGPoint) {
  CGEvent(
    mouseEventSource: eventSource,
    mouseType: .mouseMoved,
    mouseCursorPosition: start,
    mouseButton: .left
  )?.post(tap: .cghidEventTap)
  pause(milliseconds: 20)
  let down = CGEvent(
    mouseEventSource: eventSource,
    mouseType: .leftMouseDown,
    mouseCursorPosition: start,
    mouseButton: .left
  )
  down?.setIntegerValueField(.mouseEventClickState, value: 1)
  down?.post(tap: .cghidEventTap)
  for step in 1...12 {
    let fraction = CGFloat(step) / 12
    let point = CGPoint(
      x: start.x + (end.x - start.x) * fraction,
      y: start.y + (end.y - start.y) * fraction
    )
    CGEvent(
      mouseEventSource: eventSource,
      mouseType: .leftMouseDragged,
      mouseCursorPosition: point,
      mouseButton: .left
    )?.post(tap: .cghidEventTap)
    pause(milliseconds: 8)
  }
  let up = CGEvent(
    mouseEventSource: eventSource,
    mouseType: .leftMouseUp,
    mouseCursorPosition: end,
    mouseButton: .left
  )
  up?.setIntegerValueField(.mouseEventClickState, value: 1)
  up?.post(tap: .cghidEventTap)
  pause(milliseconds: 60)
}

func scrollDown(deltaY: Int, in window: (origin: CGPoint, size: CGSize, number: Int)) {
  let event = CGEvent(
    scrollWheelEvent2Source: eventSource,
    units: .pixel,
    wheelCount: 1,
    wheel1: Int32(-deltaY),
    wheel2: 0,
    wheel3: 0
  )
  event?.location = CGPoint(
    x: window.origin.x + window.size.width / 2,
    y: window.origin.y + window.size.height / 2
  )
  event?.post(tap: .cghidEventTap)
  pause(milliseconds: 100)
}

func typeText(_ value: String, intervalMs: Int) {
  for character in value {
    var units = Array(String(character).utf16)
    let down = CGEvent(
      keyboardEventSource: eventSource,
      virtualKey: 0,
      keyDown: true
    )!
    units.withUnsafeMutableBufferPointer { buffer in
      down.keyboardSetUnicodeString(
        stringLength: buffer.count,
        unicodeString: buffer.baseAddress
      )
    }
    down.post(tap: .cghidEventTap)
    let up = CGEvent(
      keyboardEventSource: eventSource,
      virtualKey: 0,
      keyDown: false
    )!
    units.withUnsafeMutableBufferPointer { buffer in
      up.keyboardSetUnicodeString(
        stringLength: buffer.count,
        unicodeString: buffer.baseAddress
      )
    }
    up.post(tap: .cghidEventTap)
    pause(milliseconds: intervalMs)
  }
}

func pressKey(_ virtualKey: CGKeyCode, flags: CGEventFlags = []) {
  let down = CGEvent(
    keyboardEventSource: eventSource,
    virtualKey: virtualKey,
    keyDown: true
  )
  down?.flags = flags
  down?.post(tap: .cghidEventTap)
  pause(milliseconds: 10)
  let up = CGEvent(
    keyboardEventSource: eventSource,
    virtualKey: virtualKey,
    keyDown: false
  )
  up?.flags = flags
  up?.post(tap: .cghidEventTap)
}

func pressCommandShortcut(_ virtualKey: CGKeyCode, shift: Bool = false) {
  let leftCommand = CGEventFlags(
    rawValue: CGEventFlags.maskCommand.rawValue | 0x00000008
  )
  let leftCommandShift = CGEventFlags(
    rawValue: leftCommand.rawValue |
      CGEventFlags.maskShift.rawValue |
      0x00000002
  )
  func modifier(_ key: CGKeyCode, down: Bool, flags: CGEventFlags) {
    let event = CGEvent(
      keyboardEventSource: eventSource,
      virtualKey: key,
      keyDown: down
    )
    event?.type = .flagsChanged
    event?.flags = flags
    event?.post(tap: .cghidEventTap)
    pause(milliseconds: 10)
  }

  modifier(55, down: true, flags: leftCommand)
  if shift {
    modifier(56, down: true, flags: leftCommandShift)
  }
  pressKey(
    virtualKey,
    flags: shift ? leftCommandShift : leftCommand
  )
  if shift {
    modifier(56, down: false, flags: leftCommand)
  }
  modifier(55, down: false, flags: [])
}

func postKey(named name: String) throws {
  switch name {
  case "enter": pressKey(36)
  case "backspace": pressKey(51)
  case "delete": pressKey(117)
  case "selectAll": pressCommandShortcut(0)
  case "copy": pressCommandShortcut(8)
  case "cut": pressCommandShortcut(7)
  case "paste": pressCommandShortcut(9)
  case "undo": pressCommandShortcut(6)
  case "redo": pressCommandShortcut(6, shift: true)
  default: throw ActuatorFailure.message("unsupported key \(name)")
  }
}

func writeJSONLine(_ value: [String: Any]) {
  do {
    let data = try JSONSerialization.data(withJSONObject: value)
    print(String(decoding: data, as: UTF8.self))
    fflush(stdout)
  } catch {
    fputs("actuator could not serialize response: \(error)\n", stderr)
  }
}

guard CommandLine.arguments.count == 3 else {
  fputs(
    "usage: live_editor_macos_canary.swift APP_EXECUTABLE LIBRARY\n",
    stderr
  )
  exit(64)
}

let appExecutable = URL(fileURLWithPath: CommandLine.arguments[1]).path
let libraryPath = URL(fileURLWithPath: CommandLine.arguments[2]).path
let harnessDirectory = URL(fileURLWithPath: NSTemporaryDirectory())
  .appendingPathComponent("flark-native-actuator-\(UUID().uuidString)")
try FileManager.default.createDirectory(
  at: harnessDirectory,
  withIntermediateDirectories: true
)
let commandPath = harnessDirectory.appendingPathComponent("command.json").path
let receiptPath = harnessDirectory.appendingPathComponent("receipt.json").path

let appProcess = Process()
appProcess.executableURL = URL(fileURLWithPath: appExecutable)
var environment = ProcessInfo.processInfo.environment
environment["FLARK_V4_LIBRARY_PATH"] = libraryPath
environment["FLARK_CANARY_COMMAND_PATH"] = commandPath
environment["FLARK_CANARY_RECEIPT_PATH"] = receiptPath
appProcess.environment = environment
appProcess.standardOutput = FileHandle.nullDevice
appProcess.standardError = FileHandle.standardError
try appProcess.run()
let appPID = appProcess.processIdentifier
try waitUntil("initial app harness receipt") {
  guard let receipt = try? readJSON(receiptPath) else { return false }
  return receipt["commandSequence"] as? Int == 0 &&
    receipt["pendingEdits"] as? Int == 0
}
var appSequence = 0

func appRequest(
  operation: String,
  arguments: [String: Any] = [:]
) throws -> [String: Any] {
  appSequence += 1
  let sequence = appSequence
  let request: [String: Any] = [
    "sequence": sequence,
    "operation": operation,
    "arguments": arguments,
  ]
  let data = try JSONSerialization.data(withJSONObject: request)
  try data.write(to: URL(fileURLWithPath: commandPath), options: .atomic)
  var receipt: [String: Any] = [:]
  try waitUntil("app receipt for request \(sequence)") {
    guard let candidate = try? readJSON(receiptPath),
      candidate["commandSequence"] as? Int == sequence
    else { return false }
    receipt = candidate
    return true
  }
  if let error = receipt["commandError"] as? String, !error.isEmpty {
    throw ActuatorFailure.message(error)
  }
  return receipt
}

func screenPoint(
  sourceUtf16Offset: Int,
  window: (origin: CGPoint, size: CGSize, number: Int)
) throws -> CGPoint {
  let receipt = try appRequest(
    operation: "lookupSourcePoint",
    arguments: ["utf16Offset": sourceUtf16Offset]
  )
  let point = try dictionary(receipt["sourcePoint"], "sourcePoint")
  guard let globalX = point["globalX"] as? Double,
    let globalY = point["globalY"] as? Double,
    let rootHeight = point["rootHeight"] as? Double
  else {
    throw ActuatorFailure.message("source point contained invalid geometry")
  }
  let contentTopInset = max(0, window.size.height - rootHeight)
  return CGPoint(
    x: window.origin.x + globalX,
    y: window.origin.y + contentTopInset + globalY
  )
}

func taskCheckboxScreenPoint(
  targetUtf16: Int,
  window: (origin: CGPoint, size: CGSize, number: Int)
) throws -> CGPoint {
  let receipt = try appRequest(
    operation: "lookupTaskCheckboxPoint",
    arguments: ["targetUtf16": targetUtf16]
  )
  let point = try dictionary(receipt["taskActionPoint"], "taskActionPoint")
  guard let globalX = point["globalX"] as? Double,
    let globalY = point["globalY"] as? Double,
    let rootHeight = point["rootHeight"] as? Double
  else {
    throw ActuatorFailure.message("task action point contained invalid geometry")
  }
  let contentTopInset = max(0, window.size.height - rootHeight)
  return CGPoint(
    x: window.origin.x + globalX,
    y: window.origin.y + contentTopInset + globalY
  )
}

var shouldStop = false
while !shouldStop, let line = readLine() {
  var sequence = 0
  do {
    let data = Data(line.utf8)
    let request = try dictionary(
      JSONSerialization.jsonObject(with: data),
      "actuator request"
    )
    sequence = try integer(request["sequence"], "sequence")
    let operation = try string(request["operation"], "operation")
    let arguments = try dictionary(request["arguments"], "arguments")
    var response: [String: Any] = [
      "schema": "flark.native-actuator/v1",
      "sequence": sequence,
      "ok": true,
      "appPid": Int(appPID),
      "platform": "macos",
    ]

    switch operation {
    case "reset":
      response["snapshot"] = try appRequest(
        operation: "reset",
        arguments: arguments
      )
    case "settle":
      response["snapshot"] = try appRequest(
        operation: "settle"
      )
    case "activateAtUtf16":
      let offset = try integer(arguments["utf16Offset"], "utf16Offset")
      let window = try focusWindow(pid: appPID)
      let point = try screenPoint(sourceUtf16Offset: offset, window: window)
      click(point)
      let activationReceipt = try appRequest(operation: "settle")
      let actualBase = activationReceipt["selectionBaseUtf16"] as? Int
      let actualExtent = activationReceipt["selectionExtentUtf16"] as? Int
      guard actualBase == offset, actualExtent == offset
      else {
        throw ActuatorFailure.message(
          "activation did not settle at source offset \(offset); " +
            "actual=\(String(describing: actualBase)).." +
            "\(String(describing: actualExtent)); " +
            "point=\(point); window=\(window.origin)/\(window.size)"
        )
      }
      response["snapshot"] = activationReceipt
    case "selectSourceRange":
      let base = try integer(arguments["baseUtf16"], "baseUtf16")
      let extent = try integer(arguments["extentUtf16"], "extentUtf16")
      let window = try focusWindow(pid: appPID)
      drag(
        from: try screenPoint(sourceUtf16Offset: base, window: window),
        to: try screenPoint(sourceUtf16Offset: extent, window: window)
      )
    case "insertText":
      typeText(
        try string(arguments["text"], "text"),
        intervalMs: try integer(arguments["cadenceMs"], "cadenceMs")
      )
    case "key":
      let key = try string(arguments["key"], "key")
      let pasteboardChange = NSPasteboard.general.changeCount
      try postKey(named: key)
      if key == "copy" || key == "cut" {
        try waitUntil("macOS pasteboard (key)") {
          NSPasteboard.general.changeCount != pasteboardChange
        }
      }
    case "pasteText":
      let text = try string(arguments["text"], "text")
      NSPasteboard.general.clearContents()
      guard NSPasteboard.general.setString(text, forType: .string) else {
        throw ActuatorFailure.message("could not set the macOS pasteboard")
      }
      pressCommandShortcut(9)
    case "toggleTaskAtUtf16":
      let target = try integer(arguments["targetUtf16"], "targetUtf16")
      let window = try focusWindow(pid: appPID)
      click(try taskCheckboxScreenPoint(targetUtf16: target, window: window))
    case "scrollBy":
      let deltaY = try integer(arguments["deltaY"], "deltaY")
      let window = try focusWindow(pid: appPID)
      scrollDown(deltaY: deltaY, in: window)
    case "pause":
      pause(milliseconds: try integer(arguments["milliseconds"], "milliseconds"))
    case "stop":
      shouldStop = true
    default:
      throw ActuatorFailure.message("unsupported actuator operation \(operation)")
    }
    writeJSONLine(response)
  } catch {
    writeJSONLine([
      "schema": "flark.native-actuator/v1",
      "sequence": sequence,
      "ok": false,
      "error": String(describing: error),
      "appPid": Int(appPID),
      "platform": "macos",
    ])
  }
}

if appProcess.isRunning {
  appProcess.terminate()
  appProcess.waitUntilExit()
}
try? FileManager.default.removeItem(at: harnessDirectory)
