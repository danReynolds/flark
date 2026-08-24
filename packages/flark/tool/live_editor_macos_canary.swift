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

func pause(microseconds: Int) {
  usleep(useconds_t(max(0, microseconds)))
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

  let screenSize = NSScreen.main?.frame.size ?? CGSize(width: 1920, height: 1080)
  var requestedOrigin = CGPoint(
    x: max(0, (screenSize.width - CGFloat(width)) / 2),
    y: max(0, (screenSize.height - CGFloat(height)) / 2)
  )
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

  let systemWide = AXUIElementCreateSystemWide()
  try waitUntil("focused dogfood accessibility target", timeoutSeconds: 60) {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(
      systemWide,
      kAXFocusedUIElementAttribute as CFString,
      &value
    ) == .success,
      let focused = value as! AXUIElement?
    else { return false }
    var focusedPID: pid_t = 0
    return AXUIElementGetPid(focused, &focusedPID) == .success &&
      focusedPID == pid
  }

  var resolvedWindow: (number: Int, bounds: CGRect)?
  try waitUntil("dogfood window geometry") {
    guard let window = cgWindow(pid: pid),
      abs(window.bounds.width - requestedSize.width) <= 1,
      abs(window.bounds.height - requestedSize.height) <= 1
    else { return false }
    resolvedWindow = window
    return true
  }
  guard let resolvedWindow else {
    throw ActuatorFailure.message("dogfood window geometry was unavailable")
  }

  return (
    resolvedWindow.bounds.origin,
    resolvedWindow.bounds.size,
    resolvedWindow.number
  )
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

func typeText(_ value: String, intervalMicros: Int) {
  let started = DispatchTime.now().uptimeNanoseconds
  for (index, character) in value.enumerated() {
    waitForSchedule(started: started, index: index, intervalMicros: intervalMicros)
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
  }
}

func waitForSchedule(
  started: UInt64,
  index: Int,
  intervalMicros: Int
) {
  if index == 0 || intervalMicros == 0 { return }
  let target = started + UInt64(index * intervalMicros) * 1_000
  while true {
    let now = DispatchTime.now().uptimeNanoseconds
    if now >= target { return }
    let remaining = target - now
    pause(microseconds: Int(min(remaining / 1_000, 2_000)))
  }
}

func repeatKey(_ name: String, count: Int, intervalMicros: Int) throws {
  let started = DispatchTime.now().uptimeNanoseconds
  for index in 0..<count {
    waitForSchedule(started: started, index: index, intervalMicros: intervalMicros)
    try postKey(named: name)
  }
}

func typeStructuralBursts(count: Int, intervalMicros: Int) throws {
  let started = DispatchTime.now().uptimeNanoseconds
  for index in 0..<count {
    waitForSchedule(started: started, index: index, intervalMicros: intervalMicros)
    try postKey(named: "enter")
    typeText("x", intervalMicros: 0)
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

/// Exercises AppKit's real dead-key/composition route rather than injecting a
/// precomposed Unicode scalar. Key codes are the physical E key and left
/// Option key; the current macOS input source resolves Option-E, E to `é`.
func pressAcuteE() {
  let leftOption = CGEventFlags(
    rawValue: CGEventFlags.maskAlternate.rawValue | 0x00000020
  )
  func modifier(_ down: Bool, flags: CGEventFlags) {
    let event = CGEvent(
      keyboardEventSource: eventSource,
      virtualKey: 58,
      keyDown: down
    )
    event?.type = .flagsChanged
    event?.flags = flags
    event?.post(tap: .cghidEventTap)
    pause(milliseconds: 10)
  }

  modifier(true, flags: leftOption)
  pressKey(14, flags: leftOption)
  modifier(false, flags: [])
  pressKey(14)
}

func postKey(named name: String) throws {
  switch name {
  case "enter": pressKey(36)
  case "backspace": pressKey(51)
  case "delete": pressKey(117)
  case "left": pressKey(123)
  case "right": pressKey(124)
  case "selectAll": pressCommandShortcut(0)
  case "copy": pressCommandShortcut(8)
  case "cut": pressCommandShortcut(7)
  case "paste": pressCommandShortcut(9)
  case "undo": pressCommandShortcut(6)
  case "redo": pressCommandShortcut(6, shift: true)
  case "acuteE": pressAcuteE()
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

if CommandLine.arguments.count == 2 &&
  CommandLine.arguments[1] == "--delivery-self-test"
{
  do {
    try runInputDeliverySelfTest()
    print("input delivery self-test PASS")
    exit(0)
  } catch {
    fputs("input delivery self-test FAIL: \(error)\n", stderr)
    exit(1)
  }
}

guard CommandLine.arguments.count == 3 || CommandLine.arguments.count == 4 else {
  fputs(
    "usage: live_editor_macos_canary.swift APP_EXECUTABLE LIBRARY " +
      "[INITIAL_PRESET]\n",
    stderr
  )
  exit(64)
}

let appExecutable = URL(fileURLWithPath: CommandLine.arguments[1]).path
let libraryPath = URL(fileURLWithPath: CommandLine.arguments[2]).path
let initialPreset = CommandLine.arguments.count == 4
  ? CommandLine.arguments[3]
  : nil
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
if let initialPreset {
  environment["FLARK_CANARY_INITIAL_PRESET"] = initialPreset
}
environment["FLARK_CANARY_PROCESS_LAUNCH_EPOCH_MICROS"] = String(
  Int(Date().timeIntervalSince1970 * 1_000_000)
)
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

func verifyExpectedSelection(_ arguments: [String: Any]) throws -> [String: Any] {
  let receipt = try appRequest(operation: "settle")
  guard let expectedBase = arguments["expectedBaseUtf16"] as? Int,
    let expectedExtent = arguments["expectedExtentUtf16"] as? Int
  else { return receipt }
  let actualBase = receipt["selectionBaseUtf16"] as? Int
  let actualExtent = receipt["selectionExtentUtf16"] as? Int
  guard actualBase == expectedBase, actualExtent == expectedExtent else {
    throw ActuatorFailure.message(
      "input target selection drifted before delivery; expected=" +
        "\(expectedBase)..\(expectedExtent), actual=" +
        "\(String(describing: actualBase)).." +
        "\(String(describing: actualExtent))"
    )
  }
  return receipt
}

func inputDeliveryAcknowledgement(
  baselineReceipt: [String: Any],
  candidateReceipt: [String: Any],
  operation: String,
  expectedGenerationAdvance: Int,
  terminalEventPrefix: String? = nil
) throws -> [String: Any]? {
  guard let baselineOrdinal = baselineReceipt["inputEventOrdinal"] as? Int,
    let baselineGeneration = baselineReceipt["sourceGeneration"] as? Int,
    let candidateOrdinal = candidateReceipt["inputEventOrdinal"] as? Int,
    let candidateGeneration = candidateReceipt["sourceGeneration"] as? Int,
    let events = candidateReceipt["inputEvents"] as? [String]
  else {
    throw ActuatorFailure.message(
      "\(operation) input acknowledgement is missing or malformed"
    )
  }
  guard expectedGenerationAdvance >= 0 else {
    throw ActuatorFailure.message(
      "\(operation) has a negative expected generation advance"
    )
  }
  guard candidateOrdinal > baselineOrdinal, let terminalEvent = events.last else {
    return nil
  }
  let targetGeneration = baselineGeneration + expectedGenerationAdvance
  guard candidateGeneration == targetGeneration else { return nil }
  guard terminalEvent.contains("generation=\(targetGeneration)") else {
    return nil
  }
  if let terminalEventPrefix,
    !terminalEvent.contains(":\(terminalEventPrefix):")
  {
    return nil
  }
  return [
    "operation": operation,
    "baselineInputEventOrdinal": baselineOrdinal,
    "terminalInputEventOrdinal": candidateOrdinal,
    "baselineSourceGeneration": baselineGeneration,
    "terminalSourceGeneration": candidateGeneration,
    "expectedGenerationAdvance": expectedGenerationAdvance,
    "terminalEvent": terminalEvent,
  ]
}

func runInputDeliverySelfTest() throws {
  let baseline: [String: Any] = [
    "inputEventOrdinal": 10,
    "sourceGeneration": 4,
    "inputEvents": ["100:accepted-deltas:generation=4"],
  ]
  let noDelivery = try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: baseline,
    operation: "batch",
    expectedGenerationAdvance: 2
  )
  guard noDelivery == nil else {
    throw ActuatorFailure.message("no-delivery control acknowledged")
  }
  let unrelated: [String: Any] = [
    "inputEventOrdinal": 11,
    "sourceGeneration": 4,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:shortcut:copy:generation=4",
    ],
  ]
  guard try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: unrelated,
    operation: "batch",
    expectedGenerationAdvance: 2
  ) == nil else {
    throw ActuatorFailure.message("unrelated event acknowledged")
  }
  let partial: [String: Any] = [
    "inputEventOrdinal": 11,
    "sourceGeneration": 5,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:accepted-deltas:generation=5",
    ],
  ]
  guard try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: partial,
    operation: "batch",
    expectedGenerationAdvance: 2
  ) == nil else {
    throw ActuatorFailure.message("partial batch acknowledged")
  }
  let terminal: [String: Any] = [
    "inputEventOrdinal": 12,
    "sourceGeneration": 6,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:accepted-deltas:generation=5",
      "120:accepted-deltas:generation=6",
    ],
  ]
  guard try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: terminal,
    operation: "batch",
    expectedGenerationAdvance: 2
  ) != nil else {
    throw ActuatorFailure.message("terminal batch was not acknowledged")
  }
  let copied: [String: Any] = [
    "inputEventOrdinal": 11,
    "sourceGeneration": 4,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:completed-copy:generation=4",
    ],
  ]
  guard try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: copied,
    operation: "copy",
    expectedGenerationAdvance: 0,
    terminalEventPrefix: "completed-copy"
  ) != nil else {
    throw ActuatorFailure.message("nonmutating terminal event was not acknowledged")
  }
  do {
    _ = try inputDeliveryAcknowledgement(
      baselineReceipt: baseline,
      candidateReceipt: ["inputEvents": []],
      operation: "malformed",
      expectedGenerationAdvance: 1
    )
    throw ActuatorFailure.message("malformed acknowledgement did not fail")
  } catch ActuatorFailure.message(let message) {
    guard message.contains("missing or malformed") else {
      throw ActuatorFailure.message(message)
    }
  }
}

func waitForInputDelivery(
  after baselineReceipt: [String: Any],
  operation: String,
  expectedGenerationAdvance: Int,
  terminalEventPrefix: String? = nil
) throws -> [String: Any] {
  var acknowledgement: [String: Any]?
  try waitUntil("app input delivery for \(operation)") {
    guard let receipt = try? readJSON(receiptPath) else { return false }
    acknowledgement = try inputDeliveryAcknowledgement(
      baselineReceipt: baselineReceipt,
      candidateReceipt: receipt,
      operation: operation,
      expectedGenerationAdvance: expectedGenerationAdvance,
      terminalEventPrefix: terminalEventPrefix
    )
    return acknowledgement != nil
  }
  return acknowledgement!
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
    case "prepareObservationWindow":
      _ = try focusWindow(
        pid: appPID,
        width: try integer(arguments["windowWidth"], "windowWidth"),
        height: try integer(arguments["windowHeight"], "windowHeight")
      )
      response["snapshot"] = try appRequest(operation: "beginObservation")
    case "selectPreset":
      response["snapshot"] = try appRequest(
        operation: "selectPreset",
        arguments: arguments
      )
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
      let window = try focusWindow(
        pid: appPID,
        width: arguments["windowWidth"] as? Int ?? 800,
        height: arguments["windowHeight"] as? Int ?? 632
      )
      let point = try screenPoint(sourceUtf16Offset: offset, window: window)
      click(point)
      let activationReceipt = try appRequest(operation: "settle")
      let actualBase = activationReceipt["selectionBaseUtf16"] as? Int
      let actualExtent = activationReceipt["selectionExtentUtf16"] as? Int
      guard actualBase == offset, actualExtent == offset
      else {
        let events = activationReceipt["inputEvents"] as? [String] ?? []
        throw ActuatorFailure.message(
          "activation did not settle at source offset \(offset); " +
            "actual=\(String(describing: actualBase)).." +
            "\(String(describing: actualExtent)); " +
            "point=\(point); window=\(window.origin)/\(window.size); " +
            "events=\(events)"
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
      _ = try focusWindow(
        pid: appPID,
        width: arguments["windowWidth"] as? Int ?? 800,
        height: arguments["windowHeight"] as? Int ?? 632
      )
      let baselineReceipt = try verifyExpectedSelection(arguments)
      typeText(
        try string(arguments["text"], "text"),
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        )
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: "insertText",
        expectedGenerationAdvance: try string(arguments["text"], "text").count
      )
    case "closeSession":
      response["snapshot"] = try appRequest(operation: "closeSession")
    case "repeatKey":
      _ = try focusWindow(
        pid: appPID,
        width: arguments["windowWidth"] as? Int ?? 800,
        height: arguments["windowHeight"] as? Int ?? 632
      )
      let baselineReceipt = try verifyExpectedSelection(arguments)
      try repeatKey(
        try string(arguments["key"], "key"),
        count: try integer(arguments["count"], "count"),
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        )
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: "repeatKey",
        expectedGenerationAdvance: try integer(arguments["count"], "count")
      )
    case "structuralBursts":
      _ = try focusWindow(
        pid: appPID,
        width: arguments["windowWidth"] as? Int ?? 800,
        height: arguments["windowHeight"] as? Int ?? 632
      )
      let baselineReceipt = try verifyExpectedSelection(arguments)
      try typeStructuralBursts(
        count: try integer(arguments["count"], "count"),
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        )
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: "structuralBursts",
        expectedGenerationAdvance:
          try integer(arguments["count"], "count") * 2
      )
    case "key":
      _ = try focusWindow(
        pid: appPID,
        width: arguments["windowWidth"] as? Int ?? 800,
        height: arguments["windowHeight"] as? Int ?? 632
      )
      let baselineReceipt = try verifyExpectedSelection(arguments)
      let key = try string(arguments["key"], "key")
      let pasteboardChange = NSPasteboard.general.changeCount
      try postKey(named: key)
      if key == "copy" || key == "cut" {
        try waitUntil("macOS pasteboard (key)") {
          NSPasteboard.general.changeCount != pasteboardChange
        }
      }
      let nonMutating =
        key == "copy" || key == "selectAll" || key == "left" || key == "right"
      let terminalPrefix: String? = switch key {
      case "copy": "completed-copy"
      case "selectAll": "completed-select-all"
      case "cut": "completed-cut"
      case "paste": "completed-paste"
      case "undo": "completed-undo"
      case "redo": "completed-redo"
      case "left", "right": "completed-navigation"
      default: nil
      }
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: "key:\(key)",
        expectedGenerationAdvance: nonMutating ? 0 : 1,
        terminalEventPrefix: terminalPrefix
      )
    case "pasteText":
      _ = try focusWindow(
        pid: appPID,
        width: arguments["windowWidth"] as? Int ?? 800,
        height: arguments["windowHeight"] as? Int ?? 632
      )
      let baselineReceipt = try verifyExpectedSelection(arguments)
      let text = try string(arguments["text"], "text")
      NSPasteboard.general.clearContents()
      guard NSPasteboard.general.setString(text, forType: .string) else {
        throw ActuatorFailure.message("could not set the macOS pasteboard")
      }
      pressCommandShortcut(9)
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: "pasteText",
        expectedGenerationAdvance: 1,
        terminalEventPrefix: "completed-paste"
      )
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
