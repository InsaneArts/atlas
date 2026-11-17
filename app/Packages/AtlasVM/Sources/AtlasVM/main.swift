import AppKit
import AtlasVMKit
import Foundation

// As `ssh`'s password helper, it prints the password the running Atlas VM
// serves it on the socket it names, and nothing else.
if let socket = ProcessInfo.processInfo.environment["ATLAS_VM_ASKPASS"] {
  exit(Askpass.answer(from: URL(filePath: socket)) ? 0 : 1)
}

// Atlas VM runs one VM for Atlas, as that VM's launchd job starts it:
// `AtlasVM serve <desktop>`. No Dock icon; a window only while the VM's
// input is the person's.
/// What it was started with.
let arguments = CommandLine.arguments
/// Where its VMs are kept.
let parent = VMFolder.parent(home: FileManager.default.homeDirectoryForCurrentUser)
// Only a removed VM's passwords deleted, under its removal; no VM started.
if ForgetPasswordsEntry.asked(arguments) {
  exit(ForgetPasswordsEntry.run(arguments, parent: parent))
}
guard arguments.count == 3, arguments[1] == "serve", let folder = VMFolder(in: parent, desktop: arguments[2]) else {
  FileHandle.standardError
    .write(Data("usage: AtlasVM serve <desktop>\n".utf8))
  exit(64)
}
/// The desktop whose VM it runs.
let desktop = arguments[2]
// A peer gone mid-write fails that write, never this run, whose end is its VM's.
signal(SIGPIPE, SIG_IGN)
/// Its application, which shows no Dock icon.
let application = NSApplication.shared
application.setActivationPolicy(.accessory)
Task { @MainActor in
  do {
    try await Runner.serve(folder, desktop: desktop)
  } catch {
    FileHandle.standardError
      .write(Data("\(error)\n".utf8))
    exit(1)
  }
}
application.run()
