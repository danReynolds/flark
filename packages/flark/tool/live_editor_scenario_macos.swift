import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

enum ScenarioFailure: Error, CustomStringConvertible {
  case message(String)

  var description: String {
    switch self {
    case .message(let value): value
    }
  }
}

func dictionary(_ value: Any?, _ label: String) throws -> [String: Any] {
  guard let result = value as? [String: Any] else {
    throw ScenarioFailure.message("missing object \(label)")
  }
  return result
}

func array(_ value: Any?, _ label: String) throws -> [Any] {
  guard let result = value as? [Any] else {
    throw ScenarioFailure.message("missing array \(label)")
  }
  return result
}

func string(_ value: Any?, _ label: String) throws -> String {
  guard let result = value as? String else {
    throw ScenarioFailure.message("missing string \(label)")
  }
  return result
}

func integer(_ value: Any?, _ label: String) throws -> Int {
  guard let result = value as? Int else {
    throw ScenarioFailure.message("missing integer \(label)")
  }
  return result
}

func pause(milliseconds: Int) {
  usleep(useconds_t(max(0, milliseconds) * 1_000))
}

func readJSON(_ path: String) throws -> [String: Any] {
  let data = try Data(contentsOf: URL(fileURLWithPath: path))
  return try dictionary(
    JSONSerialization.jsonObject(with: data),
    "JSON document"
  )
}

func waitUntil(
  _ label: String = "condition",
  timeoutSeconds: TimeInterval = 8,
  _ predicate: () throws -> Bool
) throws {
  let deadline = Date().addingTimeInterval(timeoutSeconds)
  while Date() < deadline {
    if try predicate() { return }
    pause(milliseconds: 20)
  }
  throw ScenarioFailure.message("\(label) timed out after \(timeoutSeconds)s")
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
  width: Int,
  height: Int
) throws -> (origin: CGPoint, size: CGSize, number: Int) {
  let application = AXUIElementCreateApplication(pid)
  var window: AXUIElement?
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
    window = first
    return true
  }
  guard let window else {
    throw ScenarioFailure.message("dogfood window was not accessible")
  }

  var requestedOrigin = CGPoint(x: 180, y: 100)
  var requestedSize = CGSize(width: width, height: height)
  setAXValue(
    window,
    attribute: kAXPositionAttribute,
    type: .cgPoint,
    value: &requestedOrigin
  )
  setAXValue(
    window,
    attribute: kAXSizeAttribute,
    type: .cgSize,
    value: &requestedSize
  )
  AXUIElementPerformAction(window, kAXRaiseAction as CFString)
  AXUIElementSetAttributeValue(
    window,
    kAXMainAttribute as CFString,
    kCFBooleanTrue
  )
  AXUIElementSetAttributeValue(
    window,
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

  var actualOrigin = requestedOrigin
  var actualSize = requestedSize
  var positionValue: CFTypeRef?
  if AXUIElementCopyAttributeValue(
    window,
    kAXPositionAttribute as CFString,
    &positionValue
  ) == .success,
    let value = positionValue,
    CFGetTypeID(value) == AXValueGetTypeID()
  {
    AXValueGetValue(value as! AXValue, .cgPoint, &actualOrigin)
  }
  var sizeValue: CFTypeRef?
  if AXUIElementCopyAttributeValue(
    window,
    kAXSizeAttribute as CFString,
    &sizeValue
  ) == .success,
    let value = sizeValue,
    CFGetTypeID(value) == AXValueGetTypeID()
  {
    AXValueGetValue(value as! AXValue, .cgSize, &actualSize)
  }
  return (actualOrigin, actualSize, 0)
}

let eventSource = CGEventSource(stateID: .hidSystemState)

func click(_ location: CGPoint) {
  CGEvent(
    mouseEventSource: eventSource,
    mouseType: .mouseMoved,
    mouseCursorPosition: location,
    mouseButton: .left
  )?.post(tap: .cghidEventTap)
  pause(milliseconds: 30)
  CGEvent(
    mouseEventSource: eventSource,
    mouseType: .leftMouseDown,
    mouseCursorPosition: location,
    mouseButton: .left
  )?.post(tap: .cghidEventTap)
  pause(milliseconds: 20)
  CGEvent(
    mouseEventSource: eventSource,
    mouseType: .leftMouseUp,
    mouseCursorPosition: location,
    mouseButton: .left
  )?.post(tap: .cghidEventTap)
  pause(milliseconds: 100)
}

func typeText(_ value: String, intervalMs: Int) {
  for scalar in value.unicodeScalars {
    var units = Array(String(scalar).utf16)
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
    CGEvent(
      keyboardEventSource: eventSource,
      virtualKey: 0,
      keyDown: false
    )?.post(tap: .cghidEventTap)
    pause(milliseconds: intervalMs)
  }
}

func pressReturn() {
  CGEvent(
    keyboardEventSource: eventSource,
    virtualKey: 36,
    keyDown: true
  )?.post(tap: .cghidEventTap)
  pause(milliseconds: 10)
  CGEvent(
    keyboardEventSource: eventSource,
    virtualKey: 36,
    keyDown: false
  )?.post(tap: .cghidEventTap)
}

func captureWindow(
  origin: CGPoint,
  size: CGSize,
  number: Int,
  path: String
) {
  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
  process.arguments = number == 0
    ? [
        "-x",
        "-R\(Int(origin.x)),\(Int(origin.y)),\(Int(size.width)),\(Int(size.height))",
        path,
      ]
    : ["-x", "-l\(number)", path]
  try? process.run()
  process.waitUntilExit()
}

guard CommandLine.arguments.count == 4 else {
  fputs(
    "usage: live_editor_scenario_macos.swift SCENARIO_JSON APP_EXECUTABLE LIBRARY\n",
    stderr
  )
  exit(64)
}

let scenarioPath = URL(fileURLWithPath: CommandLine.arguments[1]).path
let appExecutable = URL(fileURLWithPath: CommandLine.arguments[2]).path
let libraryPath = URL(fileURLWithPath: CommandLine.arguments[3]).path
let scenario = try readJSON(scenarioPath)
let scenarioID = try string(scenario["id"], "id")
let steps = try array(scenario["steps"], "steps")
let schedules = try array(scenario["schedules"], "schedules")
let expectation = try dictionary(scenario["expect"], "expect")
let hints = try dictionary(scenario["runnerHints"], "runnerHints")
let macHints = try dictionary(hints["macos"], "runnerHints.macos")
let expectedSource = try string(expectation["source"], "expect.source")
let expectedCaret = try integer(expectation["caretUtf16"], "expect.caretUtf16")
let expectedResyncs = try integer(
  expectation["resyncCount"],
  "expect.resyncCount"
)
let expectedFaulted = expectation["faulted"] as? Bool ?? false
let forbidden = try array(
  expectation["forbiddenSurfaceSubstrings"],
  "expect.forbiddenSurfaceSubstrings"
).map { try string($0, "forbidden surface substring") }

var failed = false
for scheduleValue in schedules {
  let schedule = try dictionary(scheduleValue, "schedule")
  let scheduleID = try string(schedule["id"], "schedule.id")
  let receiptPath = URL(fileURLWithPath: NSTemporaryDirectory())
    .appendingPathComponent(
      "flark-\(scenarioID)-\(scheduleID)-\(UUID().uuidString).json"
    ).path
  let screenshotPath = receiptPath.replacingOccurrences(
    of: ".json",
    with: ".png"
  )
  let process = Process()
  process.executableURL = URL(fileURLWithPath: appExecutable)
  var environment = ProcessInfo.processInfo.environment
  environment["FLARK_V4_LIBRARY_PATH"] = libraryPath
  environment["FLARK_SCENARIO_PATH"] = scenarioPath
  environment["FLARK_SCENARIO_RECEIPT_PATH"] = receiptPath
  process.environment = environment
  let watch = Date()
  var window = (origin: CGPoint.zero, size: CGSize.zero, number: 0)
  var scheduleFailed = false

  do {
    try process.run()
    let pid = process.processIdentifier
    window = try focusWindow(
      pid: pid,
      width: try integer(macHints["windowWidth"], "macos.windowWidth"),
      height: try integer(macHints["windowHeight"], "macos.windowHeight")
    )
    var initialRevision = 0
    try waitUntil("initial receipt") {
      guard let receipt = try? readJSON(receiptPath),
        receipt["scenarioId"] as? String == scenarioID,
        receipt["pendingEdits"] as? Int == 0
      else { return false }
      initialRevision = receipt["revision"] as? Int ?? 0
      return receipt["source"] as? String == scenario["initialSource"] as? String
    }

    click(
      CGPoint(
        x: window.origin.x
          + CGFloat(try integer(macHints["activationX"], "macos.activationX")),
        y: window.origin.y
          + CGFloat(try integer(macHints["activationY"], "macos.activationY"))
      )
    )
    var expectedRevision = initialRevision
    for (stepIndex, stepValue) in steps.enumerated() {
      let step = try dictionary(stepValue, "step")
      switch try string(step["type"], "step.type") {
      case "typeText":
        let text = try string(step["text"], "step.text")
        typeText(
          text,
          intervalMs: step["intervalMs"] as? Int ?? 0
        )
        expectedRevision += text.unicodeScalars.count
      case "pressReturn":
        pressReturn()
        expectedRevision += 1
      case "scheduleDelay":
        let key = try string(step["key"], "step.key")
        pause(milliseconds: try integer(schedule[key], "schedule.\(key)"))
      case "waitForIdle":
        try waitUntil("step \(stepIndex) idle receipt") {
          guard let receipt = try? readJSON(receiptPath) else { return false }
          return receipt["pendingEdits"] as? Int == 0
            && (receipt["revision"] as? Int ?? -1) >= expectedRevision
        }
      default:
        throw ScenarioFailure.message("unsupported scenario step")
      }
    }

    var finalReceipt: [String: Any] = [:]
    try waitUntil("final receipt") {
      guard let receipt = try? readJSON(receiptPath) else { return false }
      finalReceipt = receipt
      return receipt["pendingEdits"] as? Int == 0
        && (receipt["revision"] as? Int ?? -1) >= expectedRevision
    }
    var failures: [String] = []
    if finalReceipt["source"] as? String != expectedSource {
      failures.append("authoritative source differed")
    }
    if finalReceipt["caretUtf16"] as? Int != expectedCaret {
      failures.append("caret differed: \(finalReceipt["caretUtf16"] ?? "nil")")
    }
    if finalReceipt["resyncCount"] as? Int != expectedResyncs {
      failures.append("resync count differed: \(finalReceipt["resyncCount"] ?? "nil")")
    }
    let faulted = finalReceipt["status"] as? String == "faulted"
    if faulted != expectedFaulted {
      failures.append("fault status differed: \(finalReceipt["status"] ?? "nil")")
    }
    let frames = finalReceipt["surfaceFrames"] as? [String] ?? []
    for substring in forbidden where frames.contains(where: { $0.contains(substring) }) {
      failures.append("surface contained forbidden substring \(substring)")
    }
    if !failures.isEmpty {
      throw ScenarioFailure.message(failures.joined(separator: "; "))
    }
    let elapsedMs = Int(Date().timeIntervalSince(watch) * 1_000)
    let result: [String: Any] = [
      "id": scenarioID,
      "runner": "macos-native",
      "schedule": scheduleID,
      "elapsedMs": elapsedMs,
      "frames": frames.count,
      "revision": finalReceipt["revision"] ?? 0,
      "resyncs": finalReceipt["resyncCount"] ?? 0,
      "passed": true,
    ]
    let output = try JSONSerialization.data(withJSONObject: result)
    print("FLARK_SCENARIO_RESULT \(String(decoding: output, as: UTF8.self))")
  } catch {
    failed = true
    scheduleFailed = true
    captureWindow(
      origin: window.origin,
      size: window.size,
      number: window.number,
      path: screenshotPath
    )
    if let receipt = try? readJSON(receiptPath),
      let events = receipt["inputEvents"] as? [String]
    {
      fputs("input events:\n\(events.suffix(24).joined(separator: "\n"))\n", stderr)
    }
    fputs(
      "\(scenarioID) [macos-native/\(scheduleID)] failed: \(error)\n"
        + "receipt: \(receiptPath)\n"
        + "screenshot: \(screenshotPath)\n",
      stderr
    )
  }
  if process.isRunning {
    process.terminate()
    process.waitUntilExit()
  }
  if !scheduleFailed {
    try? FileManager.default.removeItem(atPath: receiptPath)
  }
}

if failed { exit(1) }
