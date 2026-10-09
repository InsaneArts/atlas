import AppKit
import Foundation
let mode = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "check"
let expected = "ATLAS-CLIPBOARD-502-DISPOSABLE-20261009"
let pb = NSPasteboard.general
if mode == "set" { pb.clearContents(); pb.setString(expected,forType:.string) }
let result: [String:Any] = ["uid":getuid(),"changeCount":pb.changeCount,"matchesAtlasSentinel":pb.string(forType:.string) == expected]
let bytes = try! JSONSerialization.data(withJSONObject:result,options:[.sortedKeys]); print(String(decoding:bytes,as:UTF8.self))
