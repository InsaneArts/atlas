import AppKit
import CoreGraphics
import Security
let duration = Double(CommandLine.arguments.dropFirst().first ?? "1800") ?? 1800
let end = Date().addingTimeInterval(duration)
let start = ProcessInfo.processInfo.systemUptime
while Date() < end {
    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    let position = CGEvent(source:nil)?.location ?? .zero
    let session = CGSessionCopyCurrentDictionary() as? [String:Any] ?? [:]
    let record: [String:Any] = [
        "time":ISO8601DateFormatter().string(from:Date()),
        "elapsed":ProcessInfo.processInfo.systemUptime-start,
        "uid":getuid(),
        "frontmostPID":NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1,
        "cursorX":position.x,"cursorY":position.y,
        "clipboardChangeCount":NSPasteboard.general.changeCount,
        "onConsole":session["kCGSSessionOnConsoleKey"] ?? NSNull(),
        "sessionUID":session["kCGSSessionUserIDKey"] ?? NSNull()
    ]
    if let bytes = try? JSONSerialization.data(withJSONObject:record,options:[.sortedKeys]) {
        FileHandle.standardOutput.write(bytes); FileHandle.standardOutput.write(Data([10]))
    }
}
