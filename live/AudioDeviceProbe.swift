import Foundation
import CoreAudio
var device=AudioDeviceID(0);var size=UInt32(MemoryLayout<AudioDeviceID>.size)
var a=AudioObjectPropertyAddress(mSelector:kAudioHardwarePropertyDefaultOutputDevice,mScope:kAudioObjectPropertyScopeGlobal,mElement:kAudioObjectPropertyElementMain)
let status=AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject),&a,0,nil,&size,&device)
var mute:UInt32=0;size=4;a=AudioObjectPropertyAddress(mSelector:kAudioDevicePropertyMute,mScope:kAudioDevicePropertyScopeOutput,mElement:kAudioObjectPropertyElementMain)
let muteStatus=AudioObjectGetPropertyData(device,&a,0,nil,&size,&mute)
var volume:Float32=0;size=4;a.mSelector=kAudioDevicePropertyVolumeScalar
let volumeStatus=AudioObjectGetPropertyData(device,&a,0,nil,&size,&volume)
let result:[String:Any]=["uid":getuid(),"defaultOutputDevice":device,"queryStatus":status,"muteStatus":muteStatus,"mute":mute,"volumeStatus":volumeStatus,"volume":volume]
print(String(decoding:try! JSONSerialization.data(withJSONObject:result,options:[.sortedKeys]),as:UTF8.self))
