import Foundation
import AppKit
import CoreGraphics
import IOKit
import Darwin
let targetPID=Int32(CommandLine.arguments[1]) ?? 0
let seconds=Double(CommandLine.arguments[2]) ?? 120
let armed=CommandLine.arguments.contains("--armed")
guard getuid()==502 && (!armed || targetPID>1) else {exit(64)}
let root=IORegistryGetRootEntry(kIOMainPortDefault)
let start=ProcessInfo.processInfo.systemUptime
var sleepSignal:String?=nil
let center=NSWorkspace.shared.notificationCenter
let token=center.addObserver(forName:NSWorkspace.willSleepNotification,object:nil,queue:.main){_ in sleepSignal="NSWorkspace.willSleep"}
func status()->(Bool?,Bool?) {
 let session=CGSessionCopyCurrentDictionary() as? [String:Any]
 let console=(session?["kCGSSessionOnConsoleKey"] as? NSNumber)?.boolValue
 let locked=(IORegistryEntryCreateCFProperty(root,"IOConsoleLocked" as CFString,kCFAllocatorDefault,0)?.takeRetainedValue() as? NSNumber)?.boolValue
 return(console,locked)
}
func emit(_ event:String,_ extra:[String:Any]=[:]) {
 var r=extra;r["event"]=event;r["uid"]=getuid();r["time"]=ISO8601DateFormatter().string(from:Date());r["uptime"]=ProcessInfo.processInfo.systemUptime;r["watcherPID"]=getpid();r["targetPID"]=targetPID;r["armed"]=armed
 if let d=try? JSONSerialization.data(withJSONObject:r,options:[.sortedKeys]) {FileHandle.standardOutput.write(d);FileHandle.standardOutput.write(Data([10]))}
}
let initial=status();emit("start",["onConsole":initial.0 as Any? ?? NSNull(),"locked":initial.1 as Any? ?? NSNull()])
if armed && (initial.0 != false || initial.1 != false) {emit("refused-to-arm");exit(78)}
while ProcessInfo.processInfo.systemUptime-start<seconds {
 RunLoop.current.run(until:Date().addingTimeInterval(0.005))
 let current=status()
 if armed && (current.0 != false || current.1 != false || sleepSignal != nil) {
  let reason=sleepSignal ?? (current.0 == true ? "helper-on-console" : (current.1 == true ? "console-locked" : "session-query-unavailable"))
  emit("transition-detected",["reason":reason,"onConsole":current.0 as Any? ?? NSNull(),"locked":current.1 as Any? ?? NSNull()])
  let result=kill(targetPID,SIGTERM);emit("helper-terminated",["killResult":result,"errno":errno]);exit(result==0 ? 0:1)
 }
}
emit("timeout-no-transition");center.removeObserver(token);IOObjectRelease(root)
