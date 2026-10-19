import AppKit
import AtlasKit
import SwiftUI

// The onboarding screens shown in the helper's account, in order.

struct AllowScreen: View {
    let model: OnboardingModel

    var body: some View {
        let asker = model.facts.asker
        Column(width: 480) {
            HStack(spacing: 14) {
                Text(String(asker.title.prefix(1)))
                    .font(.system(size: 20, weight: .semibold))
                    .frame(width: 56, height: 56)
                    .background(Circle().fill(Color(white: 0.23)))
                Image(systemName: "arrow.right").foregroundStyle(AtlasStyle.ink3)
                AppIcon(size: 72)
            }
            .accessibilityHidden(true)
            Eyebrow(text: "Atlas in \(model.facts.myName)").padding(.top, 6)
            Headline(title: "Let \(asker.title) use this account", tail: "through Atlas?")
            Lede(
                text:
                    "\(asker.title)'s agents will open apps here, click and type, while \(asker.title) keeps working in their own account. You allow this once; Atlas then starts here by itself."
            )
            Button("Allow") { Task { await model.allow() } }.atlas(large: true)
                .keyboardShortcut(.defaultAction)
                .disabled(model.working != nil)
                .padding(.top, 8)
            Working(model: model)
            Note(text: "Atlas can be removed from both accounts any time with atlas uninstall.")
        }
    }
}

struct PermissionsScreen: View {
    let model: OnboardingModel

    var body: some View {
        Column(width: 460) {
            Eyebrow(text: "Permissions")
            Headline(title: "Let Atlas see and use", tail: "this screen.")
            Lede(
                text:
                    "macOS asks you to allow Atlas twice, in this account only. Your own account is never touched."
            )
            Card {
                PermissionRow(
                    symbol: "cursorarrow", permission: .accessibility,
                    detail: "So agents can click, type and read buttons", on: model.granted(.accessibility))
                Rule()
                PermissionRow(
                    symbol: "rectangle.dashed", permission: .screenRecording,
                    detail: "So agents can take screenshots of this desktop", on: model.granted(.screenRecording))
            }
            .padding(.top, 4)
            if let next = model.nextPermission {
                Button("Allow \(next.title)") { model.openSettings(next) }.atlas(large: true)
                    .keyboardShortcut(.defaultAction)
                    .padding(.top, 6)
            }
            Note(
                text:
                    "Atlas notices each switch on its own. If macOS offers to quit and reopen Atlas, either choice is fine."
            )
        }
    }
}

/// One permission and whether it is on.
struct PermissionRow: View {
    let symbol: String
    let permission: Permission
    let detail: String
    let on: Bool

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 14))
                .frame(width: 30, height: 30)
                .background(RoundedRectangle(cornerRadius: 8).fill(AtlasStyle.fill))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(permission.title).font(.system(size: 13, weight: .semibold))
                Text(detail).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
            }
            Spacer(minLength: 8)
            Text(on ? "Allowed" : "Off").font(.system(size: 12, weight: .semibold))
                .foregroundStyle(on ? AtlasStyle.greenInk : AtlasStyle.ink3)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .accessibilityElement(children: .combine)
    }
}

/// The narrow panel that sits beside System Settings while a switch is
/// turned on, and steps aside by itself once it is.
struct GuidePanel: View {
    @Bindable var model: OnboardingModel
    let permission: Permission

    var body: some View {
        let on = model.granted(permission)
        VStack(spacing: 12) {
            Eyebrow(text: "\(permission.number) of \(Permission.allCases.count) · \(permission.title)")
            Headline(title: "Turn on Atlas.", size: 22).padding(.bottom, 4)
            StepRow(
                index: 1, title: "Find Atlas in the list", detail: "Privacy & Security › \(permission.title)",
                state: .done)
            StepRow(
                index: 2, title: "Turn its switch on", detail: "Confirm with Touch ID or your password",
                state: on ? .done : .current)
            StepRow(index: 3, title: "Come back here", detail: "Atlas notices on its own", state: on ? .done : .waiting)
            Spacer(minLength: 8)
            StatusPill(text: on ? "\(permission.title) is on" : "Waiting for \(permission.title)…", done: on)
            Button("Back") { model.guiding = nil }.atlas(.plain)
        }
        .padding(.horizontal, 26)
        .padding(.top, 44)
        .padding(.bottom, 18)
    }
}

struct HelperDoneScreen: View {
    let model: OnboardingModel

    var body: some View {
        let asker = model.facts.asker
        Column(width: 450) {
            Check(size: 64)
            Headline(
                title: "All set here.",
                tail: asker.name.isEmpty ? "Switch back to your own account." : "Switch back to \(asker.title)."
            )
            .padding(.top, 6)
            Lede(
                text:
                    "You won't need to set anything up here again. Atlas starts in this account by itself whenever it is logged in, and finishes setting up in your own account."
            )
            Button("Switch Back") { Task { await model.switchBack() } }.atlas(large: true)
                .keyboardShortcut(.defaultAction)
                .disabled(model.working != nil)
                .padding(.top, 10)
            Working(model: model)
            Note(text: "Locks this account and opens the lock screen.")
        }
    }
}
