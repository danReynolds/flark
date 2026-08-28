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

func readJSONForPolling(_ path: String) -> [String: Any]? {
  autoreleasepool {
    try? readJSON(path)
  }
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

func focusedAccessibilityTarget() -> (pid: pid_t, role: String, subrole: String) {
  let systemWide = AXUIElementCreateSystemWide()
  var value: CFTypeRef?
  guard AXUIElementCopyAttributeValue(
    systemWide,
    kAXFocusedUIElementAttribute as CFString,
    &value
  ) == .success,
    let focused = value as! AXUIElement?
  else { return (0, "unavailable", "unavailable") }
  var focusedPID: pid_t = 0
  _ = AXUIElementGetPid(focused, &focusedPID)

  func attribute(_ name: String) -> String {
    var attributeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(
      focused,
      name as CFString,
      &attributeValue
    ) == .success,
      let result = attributeValue as? String
    else { return "unavailable" }
    return result
  }

  return (
    focusedPID,
    attribute(kAXRoleAttribute),
    attribute(kAXSubroleAttribute)
  )
}

/// Preserves the editor first responder while recovering from another app
/// becoming frontmost during a long native input batch. Activating the app does
/// not rewrite AX window focus; delivery resumes only when its text area is the
/// focused accessibility target again.
func ensureDogfoodEditorFocus(pid: pid_t) throws {
  let focused = focusedAccessibilityTarget()
  if focused.pid != pid || focused.role != kAXTextAreaRole {
    NSRunningApplication(processIdentifier: pid)?.activate(
      options: [.activateAllWindows]
    )
  }
  try waitUntil("focused dogfood editor", timeoutSeconds: 2) {
    let candidate = focusedAccessibilityTarget()
    return candidate.pid == pid && candidate.role == kAXTextAreaRole
  }
}

let eventSource = CGEventSource(stateID: .hidSystemState)
var lastPrimaryPointerUpAt: Date?

func beginIndependentPrimaryPointerSequence() {
  if let lastPrimaryPointerUpAt {
    let elapsed = Date().timeIntervalSince(lastPrimaryPointerUpAt)
    let minimum = NSEvent.doubleClickInterval + 0.05
    if elapsed < minimum {
      pause(milliseconds: Int((minimum - elapsed) * 1_000) + 1)
    }
  }
}

func finishPrimaryPointerSequence() {
  lastPrimaryPointerUpAt = Date()
}

func click(_ location: CGPoint) {
  beginIndependentPrimaryPointerSequence()
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
  finishPrimaryPointerSequence()
  pause(milliseconds: 100)
}

func drag(from start: CGPoint, to end: CGPoint) {
  beginIndependentPrimaryPointerSequence()
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
  finishPrimaryPointerSequence()
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

func typeText(_ value: String, intervalMicros: Int, pid: pid_t) throws {
  let started = DispatchTime.now().uptimeNanoseconds
  for (index, character) in value.enumerated() {
    waitForSchedule(started: started, index: index, intervalMicros: intervalMicros)
    try ensureDogfoodEditorFocus(pid: pid)
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

func repeatKey(
  _ name: String,
  count: Int,
  intervalMicros: Int,
  pid: pid_t
) throws {
  let started = DispatchTime.now().uptimeNanoseconds
  for index in 0..<count {
    waitForSchedule(started: started, index: index, intervalMicros: intervalMicros)
    try ensureDogfoodEditorFocus(pid: pid)
    try postKey(named: name)
  }
}

func typeStructuralBursts(
  count: Int,
  intervalMicros: Int,
  pid: pid_t
) throws {
  let started = DispatchTime.now().uptimeNanoseconds
  for index in 0..<count {
    waitForSchedule(started: started, index: index, intervalMicros: intervalMicros)
    try ensureDogfoodEditorFocus(pid: pid)
    try postKey(named: "enter")
    try typeText("x", intervalMicros: 0, pid: pid)
  }
}

func repeatKeyThenText(
  _ name: String,
  count: Int,
  text: String,
  intervalMicros: Int,
  pid: pid_t
) throws {
  try repeatKey(
    name,
    count: count,
    intervalMicros: intervalMicros,
    pid: pid
  )
  try typeText(text, intervalMicros: intervalMicros, pid: pid)
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
  pause(milliseconds: 50)
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
  guard let receipt = readJSONForPolling(receiptPath) else { return false }
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
    guard let candidate = readJSONForPolling(receiptPath),
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
  guard candidateOrdinal > baselineOrdinal else {
    return nil
  }
  let firstRetainedOrdinal = candidateOrdinal - events.count + 1
  let firstNewIndex = max(0, baselineOrdinal + 1 - firstRetainedOrdinal)
  let newEvents = events.enumerated().dropFirst(firstNewIndex)
  let targetGeneration = baselineGeneration + expectedGenerationAdvance
  // A semantic edit can publish a provisional source generation and then its
  // certified successor before this polling process observes the receipt.
  // Treat the requested advance as a delivery floor; final source/revision
  // assertions in the Dart journey own the exact logical-edit contract.
  guard candidateGeneration >= targetGeneration else { return nil }
  guard let terminal = newEvents.last(where: { _, event in
    if let terminalEventPrefix {
      guard event.contains(":\(terminalEventPrefix):") else { return false }
      return expectedGenerationAdvance == 0 ||
        event.contains("generation=\(candidateGeneration)")
    }
    return event.contains("generation=\(candidateGeneration)")
  }) else {
    return nil
  }
  let terminalOrdinal = firstRetainedOrdinal + terminal.offset
  let terminalEvent = terminal.element
  return [
    "operation": operation,
    "baselineInputEventOrdinal": baselineOrdinal,
    "terminalInputEventOrdinal": terminalOrdinal,
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
    "inputEventOrdinal": 13,
    "sourceGeneration": 6,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:accepted-deltas:generation=5",
      "120:accepted-deltas:generation=6",
      "130:key-up",
    ],
  ]
  let terminalAcknowledgement = try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: terminal,
    operation: "batch",
    expectedGenerationAdvance: 2
  )
  guard terminalAcknowledgement?["terminalInputEventOrdinal"] as? Int == 12,
    terminalAcknowledgement?["terminalEvent"] as? String ==
      "120:accepted-deltas:generation=6"
  else {
    throw ActuatorFailure.message("terminal batch was not acknowledged")
  }
  let rolledOver: [String: Any] = [
    "inputEventOrdinal": 2000,
    "sourceGeneration": 6,
    "inputEvents": [
      "1999:accepted-deltas:generation=6",
      "2000:key-up",
    ],
  ]
  let rolloverAcknowledgement = try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: rolledOver,
    operation: "rolled-over-batch",
    expectedGenerationAdvance: 2
  )
  guard rolloverAcknowledgement?["terminalInputEventOrdinal"] as? Int == 1999
  else {
    throw ActuatorFailure.message("retained terminal event was not acknowledged")
  }
  let certifiedAfterProvisional: [String: Any] = [
    "inputEventOrdinal": 14,
    "sourceGeneration": 7,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:accepted-deltas:generation=5",
      "120:controller-generation:generation=6",
      "130:controller-generation:generation=7",
    ],
  ]
  let certifiedAcknowledgement = try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: certifiedAfterProvisional,
    operation: "certified-after-provisional",
    expectedGenerationAdvance: 2
  )
  guard certifiedAcknowledgement?["terminalSourceGeneration"] as? Int == 7,
    certifiedAcknowledgement?["terminalInputEventOrdinal"] as? Int == 14
  else {
    throw ActuatorFailure.message(
      "a certified generation after the delivery floor was not acknowledged"
    )
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
  let navigated: [String: Any] = [
    "inputEventOrdinal": 12,
    "sourceGeneration": 4,
    "inputEvents": [
      "100:accepted-deltas:generation=4",
      "110:key:KeyDownEvent:Arrow Left:meta=false",
      "120:key:KeyUpEvent:Arrow Left:meta=false",
    ],
  ]
  guard try inputDeliveryAcknowledgement(
    baselineReceipt: baseline,
    candidateReceipt: navigated,
    operation: "navigate",
    expectedGenerationAdvance: 0,
    terminalEventPrefix: "key:KeyUpEvent:Arrow Left"
  ) != nil else {
    throw ActuatorFailure.message("navigation key-up was not acknowledged")
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
  do {
    try waitUntil("app input delivery for \(operation)") {
      guard let receipt = readJSONForPolling(receiptPath) else { return false }
      acknowledgement = try inputDeliveryAcknowledgement(
        baselineReceipt: baselineReceipt,
        candidateReceipt: receipt,
        operation: operation,
        expectedGenerationAdvance: expectedGenerationAdvance,
        terminalEventPrefix: terminalEventPrefix
      )
      return acknowledgement != nil
    }
  } catch {
    let latest = readJSONForPolling(receiptPath)
    let baselineOrdinal = baselineReceipt["inputEventOrdinal"] as? Int
    let baselineGeneration = baselineReceipt["sourceGeneration"] as? Int
    let latestOrdinal = latest?["inputEventOrdinal"] as? Int
    let latestGeneration = latest?["sourceGeneration"] as? Int
    let latestSelectionBase = latest?["selectionBaseUtf16"] as? Int
    let latestSelectionExtent = latest?["selectionExtentUtf16"] as? Int
    let latestEvents = (latest?["inputEvents"] as? [String])?.suffix(4) ?? []
    let focused = focusedAccessibilityTarget()
    throw ActuatorFailure.message(
      "\(error); baselineOrdinal=\(String(describing: baselineOrdinal)); " +
        "latestOrdinal=\(String(describing: latestOrdinal)); " +
        "baselineGeneration=\(String(describing: baselineGeneration)); " +
        "latestGeneration=\(String(describing: latestGeneration)); " +
        "latestSelection=\(String(describing: latestSelectionBase)).." +
        "\(String(describing: latestSelectionExtent)); " +
        "focusedPid=\(focused.pid); focusedRole=\(focused.role); " +
        "focusedSubrole=\(focused.subrole); latestEvents=\(latestEvents)"
    )
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
func routedOperation(_ operation: String, _ arguments: [String: Any]) -> String {
  guard let routeId = arguments["routeId"] as? String, !routeId.isEmpty else {
    return operation
  }
  return "\(operation):\(routeId)"
}

func handleActuatorRequest(_ line: String) {
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
    case "resizeWindow":
      _ = try focusWindow(
        pid: appPID,
        width: try integer(arguments["width"], "width"),
        height: try integer(arguments["height"], "height")
      )
      response["snapshot"] = try appRequest(operation: "settle")
    case "refocusEditor":
      guard let other = NSRunningApplication.runningApplications(
        withBundleIdentifier: "com.apple.finder"
      ).first else {
        throw ActuatorFailure.message("Finder is unavailable for focus cycling")
      }
      other.activate(options: [.activateAllWindows])
      try waitUntil("dogfood editor focus loss", timeoutSeconds: 4) {
        NSWorkspace.shared.frontmostApplication?.processIdentifier != appPID
      }
      try ensureDogfoodEditorFocus(pid: appPID)
      response["snapshot"] = try appRequest(operation: "settle")
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
      var activationReceipt: [String: Any] = [:]
      for attempt in 1...2 {
        _ = try focusWindow(
          pid: appPID,
          width: Int(window.size.width),
          height: Int(window.size.height)
        )
        click(point)
        pause(
          milliseconds: Int(NSEvent.doubleClickInterval * 1_000) + 75
        )
        activationReceipt = try appRequest(operation: "settle")
        let actualBase = activationReceipt["selectionBaseUtf16"] as? Int
        let actualExtent = activationReceipt["selectionExtentUtf16"] as? Int
        if actualBase == offset, actualExtent == offset { break }
        if attempt == 2 {
          let events = activationReceipt["inputEvents"] as? [String] ?? []
          throw ActuatorFailure.message(
            "activation did not remain at source offset \(offset); " +
              "actual=\(String(describing: actualBase)).." +
              "\(String(describing: actualExtent)); " +
              "point=\(point); window=\(window.origin)/\(window.size); " +
              "events=\(events)"
          )
        }
      }
      response["snapshot"] = activationReceipt
    case "selectSourceRange":
      let base = try integer(arguments["baseUtf16"], "baseUtf16")
      let extent = try integer(arguments["extentUtf16"], "extentUtf16")
      let window = try focusWindow(pid: appPID)
      let basePoint = try screenPoint(sourceUtf16Offset: base, window: window)
      let extentPoint = try screenPoint(sourceUtf16Offset: extent, window: window)
      _ = try focusWindow(
        pid: appPID,
        width: Int(window.size.width),
        height: Int(window.size.height)
      )
      drag(from: basePoint, to: extentPoint)
    case "insertText":
      try ensureDogfoodEditorFocus(pid: appPID)
      let baselineReceipt = try verifyExpectedSelection(arguments)
      try typeText(
        try string(arguments["text"], "text"),
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        ),
        pid: appPID
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: routedOperation("insertText", arguments),
        expectedGenerationAdvance: try string(arguments["text"], "text").count
      )
    case "closeSession":
      response["snapshot"] = try appRequest(operation: "closeSession")
    case "repeatKey":
      try ensureDogfoodEditorFocus(pid: appPID)
      let baselineReceipt = try verifyExpectedSelection(arguments)
      try repeatKey(
        try string(arguments["key"], "key"),
        count: try integer(arguments["count"], "count"),
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        ),
        pid: appPID
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: routedOperation("repeatKey", arguments),
        expectedGenerationAdvance: try integer(arguments["count"], "count")
      )
    case "repeatKeyThenText":
      try ensureDogfoodEditorFocus(pid: appPID)
      let baselineReceipt = try verifyExpectedSelection(arguments)
      let count = try integer(arguments["count"], "count")
      let text = try string(arguments["text"], "text")
      try repeatKeyThenText(
        try string(arguments["key"], "key"),
        count: count,
        text: text,
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        ),
        pid: appPID
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: routedOperation("repeatKeyThenText", arguments),
        expectedGenerationAdvance: count + text.count
      )
    case "structuralBursts":
      try ensureDogfoodEditorFocus(pid: appPID)
      let baselineReceipt = try verifyExpectedSelection(arguments)
      try typeStructuralBursts(
        count: try integer(arguments["count"], "count"),
        intervalMicros: try integer(
          arguments["cadenceMicros"],
          "cadenceMicros"
        ),
        pid: appPID
      )
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: routedOperation("structuralBursts", arguments),
        expectedGenerationAdvance:
          try integer(arguments["count"], "count") * 2
      )
    case "key":
      try ensureDogfoodEditorFocus(pid: appPID)
      let baselineReceipt = try verifyExpectedSelection(arguments)
      let key = try string(arguments["key"], "key")
      let pasteboardChange = NSPasteboard.general.changeCount
      try ensureDogfoodEditorFocus(pid: appPID)
      try postKey(named: key)
      if key == "copy" || key == "cut" {
        try waitUntil("macOS pasteboard (key)") {
          NSPasteboard.general.changeCount != pasteboardChange
        }
      }
      let nonMutating =
        key == "copy" || key == "selectAll" || key == "left" || key == "right"
      let expectedGenerationAdvance = nonMutating ? 0 : (key == "acuteE" ? 2 : 1)
      let terminalPrefix: String? = switch key {
      case "copy": "completed-copy"
      case "selectAll": "completed-select-all"
      case "cut": "completed-cut"
      case "paste": "completed-paste"
      case "undo": "completed-undo"
      case "redo": "completed-redo"
      case "left": "key:KeyUpEvent:Arrow Left"
      case "right": "key:KeyUpEvent:Arrow Right"
      default: nil
      }
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: routedOperation("key:\(key)", arguments),
        expectedGenerationAdvance: expectedGenerationAdvance,
        terminalEventPrefix: terminalPrefix
      )
    case "pasteText":
      try ensureDogfoodEditorFocus(pid: appPID)
      let baselineReceipt = try verifyExpectedSelection(arguments)
      let text = try string(arguments["text"], "text")
      NSPasteboard.general.clearContents()
      guard NSPasteboard.general.setString(text, forType: .string) else {
        throw ActuatorFailure.message("could not set the macOS pasteboard")
      }
      try ensureDogfoodEditorFocus(pid: appPID)
      pressCommandShortcut(9)
      response["inputDeliveryAcknowledgement"] = try waitForInputDelivery(
        after: baselineReceipt,
        operation: routedOperation("pasteText", arguments),
        expectedGenerationAdvance: 1,
        terminalEventPrefix: "completed-paste"
      )
    case "toggleTaskAtUtf16":
      let target = try integer(arguments["targetUtf16"], "targetUtf16")
      let window = try focusWindow(pid: appPID)
      let point = try taskCheckboxScreenPoint(
        targetUtf16: target,
        window: window
      )
      _ = try focusWindow(
        pid: appPID,
        width: Int(window.size.width),
        height: Int(window.size.height)
      )
      click(point)
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

while !shouldStop, let line = readLine() {
  autoreleasepool {
    handleActuatorRequest(line)
  }
}

if appProcess.isRunning {
  appProcess.terminate()
  appProcess.waitUntilExit()
}
try? FileManager.default.removeItem(at: harnessDirectory)
