import AppKit
import AtlasVMKit
import Foundation

// Atlas VM runs one VM for Atlas, as that VM's launchd job starts it:
// `AtlasVM serve <desktop>`. No Dock icon; a window only while the VM's
// input is the person's.
let arguments = CommandLine.arguments
let parent = VMFolder.parent(home: FileManager.default.homeDirectoryForCurrentUser)
guard arguments.count == 3, arguments[1] == "serve", let folder = VMFolder(in: parent, desktop: arguments[2]) else {
  FileHandle.standardError.write(Data("usage: AtlasVM serve <desktop>\n".utf8))
  exit(64)
}
let desktop = arguments[2]
// A peer gone mid-write fails that write, never this run, whose end is its VM's.
signal(SIGPIPE, SIG_IGN)
let application = NSApplication.shared
application.setActivationPolicy(.accessory)
Task { @MainActor in
  do {
    try await Runner.serve(folder, desktop: desktop)
  } catch {
    FileHandle.standardError.write(Data("\(error)\n".utf8))
    exit(1)
  }
}
application.run()
