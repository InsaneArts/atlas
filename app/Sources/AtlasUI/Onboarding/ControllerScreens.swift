import AppKit
import AtlasKit
import SwiftUI

// The onboarding screens shown in the person's own account, in order.

struct WelcomeScreen: View {
    let model: OnboardingModel

    var body: some View {
        Column {
            AppIcon(size: 104)
            Headline(title: "Give your agents", tail: "a Mac of their own.").padding(.top, 8)
            Lede(
                text:
                    "Atlas runs a hidden second login on this Mac. Your agents open their builds there, click through them and come back with screenshots, while you keep working."
            )
            Button("Get Started") { model.begin() }.atlas(large: true)
                .keyboardShortcut(.defaultAction)
                .padding(.top, 10)
            Note(text: "You set this up once, in about five minutes. After that Atlas runs by itself.")
        }
    }
}

struct ChooseScreen: View {
    @Bindable var model: OnboardingModel

    var body: some View {
        Column(width: 480) {
            Eyebrow(text: "Helper account")
            Headline(title: "Agents need an account", tail: "of their own.")
            Lede(
                text:
                    "A standard account on this Mac that only Atlas uses. Your own files and windows stay out of its reach."
            )
            VStack(spacing: 8) {
                Choice(
                    title: "Create a new account",
                    detail: "Recommended. A standard account named Atlas, with Atlas's picture.",
                    symbol: "plus", selected: model.picked == ""
                ) { model.picked = "" }
                ForEach(model.facts.candidates, id: \.name) { candidate in
                    Choice(
                        title: candidate.title, detail: "\(candidate.name) · standard account on this Mac",
                        letter: String(candidate.title.prefix(1)), selected: model.picked == candidate.name
                    ) { model.picked = candidate.name }
                }
            }
            .padding(.top, 6)
            Working(model: model)
            Note(text: "Administrator accounts are not listed: a helper must be a standard account.")
        }
    }
}

/// One selectable account row.
struct Choice: View {
    let title: String
    let detail: String
    var symbol: String?
    var letter: String?
    let selected: Bool
    let pick: () -> Void

    var body: some View {
        Button(action: pick) {
            HStack(spacing: 12) {
                ZStack {
                    Circle().fill(symbol != nil ? AtlasStyle.fill : Color(white: 0.23))
                    if let symbol {
                        Image(systemName: symbol).font(.system(size: 13, weight: .semibold))
                    } else if let letter {
                        Text(letter).font(.system(size: 13, weight: .semibold))
                    }
                }
                .frame(width: 34, height: 34)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 13, weight: .semibold))
                    Text(detail).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
                }
                Spacer(minLength: 0)
                Circle()
                    .strokeBorder(selected ? AtlasStyle.blue : Color.white.opacity(0.3), lineWidth: selected ? 5 : 1.5)
                    .frame(width: 16, height: 16)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .background(
                RoundedRectangle(cornerRadius: 12).fill(selected ? AtlasStyle.blue.opacity(0.12) : AtlasStyle.card)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(
                        selected ? AtlasStyle.blue.opacity(0.55) : AtlasStyle.line, lineWidth: selected ? 1 : 0.5)
            )
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
        .foregroundStyle(AtlasStyle.ink)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

struct CreateScreen: View {
    @Bindable var model: OnboardingModel

    var body: some View {
        Column(width: 440) {
            Eyebrow(text: "New account")
            Headline(
                title: "Create the helper account.",
                tail: model.working != nil ? "Confirm in the macOS dialog." : "macOS asks for your password next.")
            Card {
                Field(label: "Full name", text: $model.fullName)
                Rule()
                Field(label: "Account name", text: $model.accountName)
                Rule()
                Field(label: "Password", text: $model.password, secure: true)
                Rule()
                Field(label: "Verify", text: $model.verify, secure: true)
            }
            .disabled(model.working != nil)
            .opacity(model.working != nil ? 0.5 : 1)
            .padding(.top, 6)
            if let mismatch = model.passwordProblem {
                Note(text: mismatch, color: AtlasStyle.orange)
            } else {
                Working(model: model)
            }
            Note(
                text:
                    "You log in to the helper with this password, now and after each restart. Atlas hands it to macOS and never keeps it."
            )
        }
    }
}

/// A labelled text field in a card.
struct Field: View {
    let label: String
    @Binding var text: String
    var secure = false

    var body: some View {
        HStack(spacing: 12) {
            Text(label).font(.system(size: 13)).foregroundStyle(AtlasStyle.ink2)
                .frame(width: 120, alignment: .leading)
            Group {
                if secure {
                    SecureField("", text: $text)
                } else {
                    TextField("", text: $text)
                }
            }
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .accessibilityLabel(label)
        }
        .padding(.horizontal, 14)
        .frame(height: 40)
    }
}

struct SwitchScreen: View {
    let model: OnboardingModel

    var body: some View {
        let helper = model.facts.helper
        let rows = switchRows(helper)
        Column(width: 460) {
            Eyebrow(text: "Helper account")
            Headline(title: "Now log in to \(helper.title).", tail: "Just this once, to set Atlas up there.")
            VStack(spacing: 8) {
                StepRow(
                    index: 1, title: "Lock and switch", detail: "Choose \(helper.title) on the lock screen",
                    state: rows[0])
                StepRow(
                    index: 2,
                    title: helper.signedIn
                        ? "Open Atlas in \(helper.title)" : "Answer the welcome questions, then open Atlas there",
                    detail: "In Applications, or with Spotlight", state: rows[1])
                StepRow(
                    index: 3, title: "Allow Atlas twice, then switch back", detail: "Atlas there shows you how",
                    state: rows[2])
            }
            .padding(.top, 4)
            if !helper.signedIn || !helper.onConsole {
                Button("Lock and Switch") { Task { await model.lockAndSwitch() } }.atlas(large: true)
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.working != nil)
            }
            if model.working != nil || model.problem != nil {
                Working(model: model)
            } else {
                StatusPill(text: waiting(helper, rows))
            }
        }
    }

    private func waiting(_ helper: OnboardingFacts.Helper, _ rows: [RowState]) -> String {
        if !helper.signedIn { return "Next: Lock and Switch, then log in to \(helper.title)." }
        if !helper.answering {
            return helper.onConsole
                ? "Waiting for Atlas to open in \(helper.title)…" : "Next: Lock and Switch, then open Atlas there."
        }
        if rows[2] != .done { return "Waiting for the two permissions…" }
        return "Waiting for you to switch back…"
    }
}

struct FinishingScreen: View {
    let model: OnboardingModel

    var body: some View {
        Column(width: 440) {
            Eyebrow(text: "Welcome back")
            Headline(
                title: model.finish == .passed ? "Everything works." : "Checking everything",
                tail: model.finish == .passed ? "Your agents have a Mac now." : "end to end.")
            Card {
                CheckLine(text: "\(model.facts.helper.title) is logged in", state: .done, note: "in the background")
                Rule()
                CheckLine(text: "Atlas runs in the helper", state: .done, note: "starts by itself")
                Rule()
                CheckLine(text: "Screen Recording and Accessibility", state: .done, note: "allowed")
                Rule()
                CheckLine(text: "Mouse, keyboard and a test screenshot", state: lineState, note: lineNote)
            }
            .padding(.top, 6)
            if case .failed(let why) = model.finish {
                Note(text: why, color: AtlasStyle.orange)
                Button("Try Again") { model.retryFinish() }.atlas(.glass)
            } else {
                Note(text: "The last check starts input in the helper, takes a screenshot and confirms it.")
            }
        }
    }

    private var lineState: CheckLine.State {
        switch model.finish {
        case .passed: .done
        case .failed: .failed
        default: .busy
        }
    }

    private var lineNote: String {
        switch model.finish {
        case .passed: "passed"
        case .failed: "failed"
        default: "checking…"
        }
    }
}

/// One line of the end-to-end check.
struct CheckLine: View {
    enum State { case done, busy, failed }
    let text: String
    let state: State
    var note = ""

    var body: some View {
        HStack(spacing: 12) {
            Group {
                switch state {
                case .done: Check()
                case .busy: Spinner(size: 14)
                case .failed: Image(systemName: "exclamationmark.circle.fill").foregroundStyle(AtlasStyle.orange)
                }
            }
            .frame(width: 18, height: 18)
            Text(text).font(.system(size: 13))
            Spacer(minLength: 8)
            Text(note).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink3)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
    }
}

struct AgentsScreen: View {
    @Bindable var model: OnboardingModel

    var body: some View {
        Column(width: 460) {
            Eyebrow(text: "Your agents")
            Headline(title: "Teach your agents", tail: "to test on screen.")
            Card {
                ForEach($model.agents) { $agent in
                    if agent.id != model.agents.first?.id { Rule() }
                    AgentRow(agent: $agent)
                }
            }
            .padding(.top, 6)
            Card {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Teach a project too").font(.system(size: 13, weight: .semibold))
                            Text("Adds a short section to its AGENTS.md, so agents there check their work on screen.")
                                .font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer(minLength: 8)
                        Button("Choose Folder…") { model.chooseProject() }.atlas(.glass)
                            .disabled(model.working != nil)
                    }
                    if !model.projects.isEmpty {
                        HStack(spacing: 6) {
                            ForEach(model.projects, id: \.self) { url in
                                ProjectChip(url: url, taught: model.taught.contains(url)) { model.removeProject(url) }
                                    .disabled(model.working != nil)
                            }
                        }
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
            }
            Working(model: model)
        }
    }
}

/// A chosen project; removable until it is taught.
struct ProjectChip: View {
    let url: URL
    let taught: Bool
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Text(url.lastPathComponent).font(.system(size: 12))
            if !taught {
                Button(action: remove) {
                    Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Remove \(url.lastPathComponent)")
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 26)
        .foregroundStyle(taught ? AtlasStyle.greenInk : AtlasStyle.ink)
        .background(Capsule().fill(taught ? AtlasStyle.green.opacity(0.12) : AtlasStyle.fill))
    }
}

/// One agent, with its switch.
struct AgentRow: View {
    @Binding var agent: OnboardingModel.AgentChoice

    var body: some View {
        HStack(spacing: 12) {
            Text(">_").font(.system(size: 12, design: .monospaced))
                .frame(width: 30, height: 30)
                .background(RoundedRectangle(cornerRadius: 8).fill(AtlasStyle.fill))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(agent.title).font(.system(size: 13, weight: .semibold))
                Text(detail).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
            }
            Spacer(minLength: 8)
            Toggle(agent.title, isOn: Binding(get: { agent.on || agent.connected }, set: { agent.on = $0 }))
                .toggleStyle(.switch)
                .labelsHidden()
                .tint(AtlasStyle.blue)
                .disabled(!agent.found || agent.connected)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }

    private var detail: String {
        if agent.connected { return "Connected. Every session has the Atlas skill." }
        if agent.found { return "Found. Adds the Atlas skill to every session." }
        return "Not installed on this Mac."
    }
}

struct DoneScreen: View {
    @Bindable var model: OnboardingModel

    var body: some View {
        Column(width: 420) {
            AppIcon(size: 92)
            Headline(title: "You're set.").padding(.top, 8)
            Lede(
                text:
                    "That was the whole setup. Atlas lives in your menu bar: ask your agent to check something on screen, and watch it work from there. After a restart, just log in to \(model.facts.helper.title) again; Atlas does the rest."
            )
            Card {
                HStack(spacing: 12) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Open Atlas at login").font(.system(size: 13, weight: .semibold))
                        Text("So Atlas is there whenever you are.").font(.system(size: 12))
                            .foregroundStyle(AtlasStyle.ink2)
                    }
                    Spacer(minLength: 8)
                    Toggle("Open Atlas at login", isOn: $model.openAtLogin)
                        .toggleStyle(.switch).labelsHidden().tint(AtlasStyle.blue)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
            }
            .padding(.top, 6)
            Working(model: model)
            Note(text: "Getting Started… in the menu bar brings this window back.")
        }
    }
}

struct BlockedScreen: View {
    let model: OnboardingModel
    let why: String

    var body: some View {
        Column(width: 440) {
            Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 40)).foregroundStyle(
                AtlasStyle.orange
            )
            .accessibilityHidden(true)
            Headline(title: "One thing first.")
            Lede(text: why)
            Button("Check Again") { model.refresh() }.atlas(large: true).padding(.top, 8)
        }
    }
}
