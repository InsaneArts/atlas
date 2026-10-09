import Foundation
import ApplicationServices
let pid = Int32(CommandLine.arguments[1])!
let root=AXUIElementCreateApplication(pid)
var seen=0
func walk(_ e:AXUIElement,_ depth:Int)->[String:Any] {
 seen += 1
 var row:[String:Any]=[:]
 for name in ["AXRole","AXTitle","AXDescription","AXValue","AXEnabled","AXPosition","AXSize"] {
  var v:CFTypeRef?;if AXUIElementCopyAttributeValue(e,name as CFString,&v) == .success,let v=v {row[name]=String(describing:v)}
 }
 if depth<12 && seen<350 {var v:CFTypeRef?;if AXUIElementCopyAttributeValue(e,kAXChildrenAttribute as CFString,&v) == .success,let children=v as? [AXUIElement] {row["children"]=children.map {walk($0,depth+1)}}}
 return row
}
let bytes=try JSONSerialization.data(withJSONObject:["trusted":AXIsProcessTrusted(),"tree":walk(root,0)],options:[.prettyPrinted,.sortedKeys]);print(String(decoding:bytes,as:UTF8.self))
