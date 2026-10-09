import Foundation
import CoreGraphics
let deadline=Date().addingTimeInterval(1200)
let helperPIDs:Set<Int>=[31563,32746,32831,31400]
while Date()<deadline {
 let list=CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]] ?? []
 let matches=list.compactMap { row -> [String:Int]? in
  guard let pid=row[kCGWindowOwnerPID as String] as? Int,helperPIDs.contains(pid) else {return nil}
  return ["pid":pid,"windowID":row[kCGWindowNumber as String] as? Int ?? -1]
 }
 let session=CGSessionCopyCurrentDictionary() as? [String:Any] ?? [:]
 let r:[String:Any]=["timestamp":ISO8601DateFormatter().string(from:Date()),"uid":getuid(),"onConsole":session["kCGSSessionOnConsoleKey"] ?? NSNull(),"helperWindowsInHostSession":matches]
 let data=try! JSONSerialization.data(withJSONObject:r,options:[.sortedKeys]);FileHandle.standardOutput.write(data);FileHandle.standardOutput.write(Data([10]));Thread.sleep(forTimeInterval:0.1)
}
