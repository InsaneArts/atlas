import AppKit
import AtlasKit
import SwiftUI

/// The window's content: one screen at a time, chosen by the model.
public struct OnboardingView: View {
    @ObservedObject var model: OnboardingModel
    let close: @MainActor () -> Void

    public init(model: OnboardingModel, close: @escaping @MainActor () -> Void) {
        self.model = model
        self.close = close
    }

    public var body: some View {
        ZStack {
            AtlasStyle.ground.ignoresSafeArea()
            if let pane = model.guiding {
                GuidePanel(model: model, pane: pane)
                    .transition(.opacity)
            } else if !model.loaded {
                VStack(spacing: 12) {
                    Spinner(size: 18)
                    if let problem = model.problem { Note(text: problem, color: AtlasStyle.orange) }
                }
            } else {
                Screen(model: model, close: close)
                    .id(model.step.key)
                    .transition(.asymmetric(
                        insertion: .opacity.combined(with: .offset(y: 14)).animation(.easeOut(duration: 0.4).delay(0.08)),
                        removal: .opacity.combined(with: .offset(y: -10)).animation(.easeOut(duration: 0.2))))
            }
        }
        .foregroundStyle(AtlasStyle.ink)
        .environment(\.colorScheme, .dark)
    }
}

extension OnboardingStep {
    var key: String {
        switch self {
        case .blocked(let why): return "blocked:\(why)"
        default: return String(describing: self)
        }
    }
}

/// One full screen: its content above, its footer below.
struct Screen: View {
    @ObservedObject var model: OnboardingModel
    let close: @MainActor () -> Void

    var body: some View {
        VStack(spacing: 0) {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.top, 36)
            footer
        }
    }

    @ViewBuilder var content: some View {
        switch model.step {
        case .welcome: WelcomeScreen(model: model)
        case .chooseHelper: ChooseScreen(model: model)
        case .createAccount: CreateScreen(model: model)
        case .switchToHelper: SwitchScreen(model: model)
        case .finishing: FinishingScreen(model: model)
        case .agents: AgentsScreen(model: model)
        case .done: DoneScreen(model: model)
        case .allow: AllowScreen(model: model)
        case .permissions: PermissionsScreen(model: model)
        case .helperDone: HelperDoneScreen(model: model)
        case .blocked(let why): BlockedScreen(model: model, why: why)
        }
    }

    var footer: some View {
        ZStack {
            if let dots = model.step.dots { Dots(index: dots.index, count: dots.count) }
            HStack {
                back
                Spacer()
                next
            }
        }
        .padding(.horizontal, 22)
        .frame(height: 76)
        .overlay(alignment: .top) { Rule() }
    }

    @ViewBuilder var back: some View {
        switch model.step {
        case .chooseHelper where !model.facts.configured:
            Button("Back") { model.choices.started = false }.atlas(.plain)
        case .createAccount:
            Button("Back") { model.choices.creating = false }.atlas(.plain)
                .disabled(model.working != nil)
        case .done:
            Button("Back") { model.choices.agentsDone = false }.atlas(.plain)
        default: EmptyView()
        }
    }

    @ViewBuilder var next: some View {
        switch model.step {
        case .chooseHelper:
            Button("Continue") { model.useHelper() }.atlas()
                .disabled(model.picked == nil || model.working != nil)
                .keyboardShortcut(.defaultAction)
        case .createAccount:
            Button("Create Account") { model.createAccount() }.atlas()
                .disabled(!model.canCreate || model.working != nil)
                .keyboardShortcut(.defaultAction)
        case .finishing:
            Button("Continue") { model.refresh() }.atlas()
                .disabled(model.finish != .passed)
        case .agents:
            Button("Continue") { model.connectAgents() }.atlas()
                .disabled(model.working != nil)
                .keyboardShortcut(.defaultAction)
        case .done:
            Button("Done") { model.complete(close) }.atlas()
                .disabled(model.working != nil)
                .keyboardShortcut(.defaultAction)
        default: EmptyView()
        }
    }
}

/// The centred column every screen uses.
struct Column<Content: View>: View {
    var width: CGFloat = 440
    @ViewBuilder let content: Content
    var body: some View {
        VStack(spacing: 14) { content }
            .frame(maxWidth: width)
            .padding(.horizontal, 24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The line under the controls: what is running, or what went wrong.
struct Working: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        Group {
            if let working = model.working {
                HStack(spacing: 8) {
                    Spinner()
                    Text(working).font(.system(size: 12.5)).foregroundStyle(AtlasStyle.ink2)
                }
            } else if let problem = model.problem {
                Note(text: problem, color: AtlasStyle.orange)
            } else {
                Color.clear.frame(width: 1, height: 1)
            }
        }
        .frame(minHeight: 20)
        .accessibilityElement(children: .combine)
    }
}

// MARK: The person's account

struct WelcomeScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        Column {
            AppIcon(size: 104)
            Headline(title: "Give your agents", tail: "a Mac of their own.").padding(.top, 8)
            Lede(text: "Atlas runs a hidden second login on this Mac. Your agents open their builds there, click through them and come back with screenshots, while you keep working.")
            Button("Get Started") { model.begin() }.atlas(large: true)
                .keyboardShortcut(.defaultAction)
                .padding(.top, 10)
            Note(text: "About five minutes. You switch accounts once.")
        }
    }
}

struct ChooseScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        Column(width: 480) {
            Eyebrow(text: "Helper account")
            Headline(title: "Agents need an account", tail: "of their own.")
            Lede(text: "A standard account on this Mac that only Atlas uses. Your own files and windows stay out of its reach.")
            VStack(spacing: 8) {
                Choice(
                    title: "Create a new account", detail: "Recommended. A standard account named Atlas Helper.",
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
                    Circle().fill(symbol != nil ? Color.white.opacity(0.08) : Color(white: 0.23))
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
            .background(RoundedRectangle(cornerRadius: 12).fill(selected ? AtlasStyle.blue.opacity(0.12) : AtlasStyle.card))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(selected ? AtlasStyle.blue.opacity(0.55) : AtlasStyle.line, lineWidth: selected ? 1 : 0.5))
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
        .foregroundStyle(AtlasStyle.ink)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

struct CreateScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        Column(width: 440) {
            Eyebrow(text: "New account")
            Headline(
                title: "Create \(model.fullName.isEmpty ? "the helper" : model.fullName).",
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
            Note(text: "You type this password once, to sign in to the helper. Atlas hands it to macOS and never keeps it.")
        }
    }
}

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
    @ObservedObject var model: OnboardingModel
    var body: some View {
        let helper = model.facts.helper
        let rows = switchRows(helper)
        Column(width: 460) {
            Eyebrow(text: "One switch")
            Headline(title: "Now sign in to \(helper.title).", tail: "Once, to set Atlas up there.")
            VStack(spacing: 8) {
                StepRow(index: 1, title: "Lock and switch", detail: "Choose \(helper.title) on the lock screen", state: rows[0])
                StepRow(
                    index: 2,
                    title: helper.signedIn ? "Open Atlas in \(helper.title)" : "Answer macOS's welcome questions, then open Atlas there",
                    detail: "In Applications, or with Spotlight", state: rows[1])
                StepRow(index: 3, title: "Allow Atlas twice, then switch back", detail: "Atlas there shows you how", state: rows[2])
            }
            .padding(.top, 4)
            if !helper.signedIn || !helper.onConsole {
                Button("Lock and Switch") { model.lockAndSwitch() }.atlas(large: true)
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

    func waiting(_ helper: OnboardingFacts.Helper, _ rows: [RowState]) -> String {
        if !helper.signedIn { return "Next: Lock and Switch, then sign in to \(helper.title)." }
        if !helper.answering {
            return helper.onConsole
                ? "Waiting for Atlas to open in \(helper.title)…"
                : "Next: Lock and Switch, then open Atlas there."
        }
        if rows[2] != .done { return "Waiting for the two permissions…" }
        return "Waiting for you to switch back…"
    }
}

struct FinishingScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        let failed: String? = {
            if case .failed(let why) = model.finish { return why }
            return nil
        }()
        Column(width: 440) {
            Eyebrow(text: "Welcome back")
            Headline(
                title: model.finish == .passed ? "Everything works." : "Checking everything",
                tail: model.finish == .passed ? "Your agents have a Mac now." : "end to end.")
            Card {
                CheckLine(text: "\(model.facts.helper.title) is signed in", state: .done, note: "in the background")
                Rule()
                CheckLine(text: "Atlas runs in the helper", state: .done, note: "starts by itself")
                Rule()
                CheckLine(text: "Screen Recording and Accessibility", state: .done, note: "allowed")
                Rule()
                CheckLine(text: "Mouse, keyboard and a test screenshot", state: lineState, note: lineNote)
            }
            .padding(.top, 6)
            if let failed {
                Note(text: failed, color: AtlasStyle.orange)
                Button("Try Again") { model.retryFinish() }.atlas(.glass)
            } else {
                Note(text: "The last check starts input in the helper, takes a screenshot and confirms it.")
            }
        }
    }

    var lineState: CheckLine.State {
        switch model.finish {
        case .passed: return .done
        case .failed: return .failed
        default: return .busy
        }
    }

    var lineNote: String {
        switch model.finish {
        case .passed: return "passed"
        case .failed: return "failed"
        default: return "checking…"
        }
    }
}

struct CheckLine: View {
    enum State { case done, busy, todo, failed }
    let text: String
    let state: State
    var note = ""
    var body: some View {
        HStack(spacing: 12) {
            Group {
                switch state {
                case .done: Check()
                case .busy: Spinner(size: 14)
                case .todo: Circle().strokeBorder(Color.white.opacity(0.22), lineWidth: 1.5).frame(width: 14, height: 14)
                case .failed: Image(systemName: "exclamationmark.circle.fill").foregroundStyle(AtlasStyle.orange)
                }
            }
            .frame(width: 18, height: 18)
            Text(text).font(.system(size: 13)).foregroundStyle(state == .todo ? AtlasStyle.ink3 : AtlasStyle.ink)
            Spacer(minLength: 8)
            Text(note).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink3)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
    }
}

struct AgentsScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        Column(width: 460) {
            Eyebrow(text: "Your agents")
            Headline(title: "Teach your agents", tail: "to test on screen.")
            Card {
                ForEach(Array(model.agents.enumerated()), id: \.element.id) { index, agent in
                    if index > 0 { Rule() }
                    AgentRow(agent: agent) { on in
                        model.agents[index].on = on
                    }
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
                    }
                    if !model.projects.isEmpty {
                        HStack(spacing: 6) {
                            ForEach(model.projects, id: \.self) { url in
                                let taught = model.taught.contains(url)
                                HStack(spacing: 6) {
                                    Text(url.lastPathComponent).font(.system(size: 12))
                                    if !taught {
                                        Button {
                                            model.removeProject(url)
                                        } label: {
                                            Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
                                        }
                                        .buttonStyle(.plain)
                                        .accessibilityLabel("Remove \(url.lastPathComponent)")
                                    }
                                }
                                .padding(.horizontal, 10)
                                .frame(height: 26)
                                .foregroundStyle(taught ? AtlasStyle.greenInk : AtlasStyle.ink)
                                .background(Capsule().fill(taught ? AtlasStyle.green.opacity(0.12) : Color.white.opacity(0.08)))
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

struct AgentRow: View {
    let agent: OnboardingModel.AgentChoice
    let toggle: (Bool) -> Void
    var body: some View {
        HStack(spacing: 12) {
            Text(">_").font(.system(size: 12, design: .monospaced))
                .frame(width: 30, height: 30)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color.white.opacity(0.08)))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(agent.title).font(.system(size: 13, weight: .semibold))
                Text(detail).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
            }
            Spacer(minLength: 8)
            Toggle(agent.title, isOn: Binding(get: { agent.on || agent.connected }, set: toggle))
                .toggleStyle(.switch)
                .labelsHidden()
                .tint(AtlasStyle.blue)
                .disabled(!agent.found || agent.connected)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }

    var detail: String {
        if agent.connected { return "Connected. Every session has the Atlas skill." }
        if agent.found { return "Found. Adds the Atlas skill to every session." }
        return "Not installed on this Mac."
    }
}

struct DoneScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        Column(width: 420) {
            AppIcon(size: 92)
            Headline(title: "You're set.").padding(.top, 8)
            Lede(text: "Atlas lives in your menu bar. Ask your agent to check something on screen, and watch it work from there.")
            Card {
                HStack(spacing: 12) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Open Atlas at login").font(.system(size: 13, weight: .semibold))
                        Text("So the menu bar is there whenever you are.").font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
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
    @ObservedObject var model: OnboardingModel
    let why: String
    var body: some View {
        Column(width: 440) {
            Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 40)).foregroundStyle(AtlasStyle.orange)
                .accessibilityHidden(true)
            Headline(title: "One thing first.")
            Lede(text: why)
            Button("Check Again") { model.refresh() }.atlas(large: true).padding(.top, 8)
        }
    }
}

// MARK: The helper's account

struct AllowScreen: View {
    @ObservedObject var model: OnboardingModel
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
            Lede(text: "\(asker.title)'s agents will open apps here, click and type, while \(asker.title) keeps working in their own account. Atlas then starts here by itself whenever this account is signed in.")
            Button("Allow") { model.allow() }.atlas(large: true)
                .keyboardShortcut(.defaultAction)
                .disabled(model.working != nil)
                .padding(.top, 8)
            Working(model: model)
            Note(text: "Atlas can be removed from both accounts any time with atlas uninstall.")
        }
    }
}

struct PermissionsScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        let asker = model.facts.asker
        Column(width: 460) {
            Eyebrow(text: "Two switches")
            Headline(title: "Let Atlas see and use", tail: "this screen.")
            Lede(text: "macOS asks you to allow Atlas twice, in this account only. Your own account is never touched.")
            Card {
                PermissionRow(symbol: "rectangle.dashed", title: "Screen Recording", detail: "So agents can take screenshots of this desktop", on: asker.screenCapture)
                Rule()
                PermissionRow(symbol: "cursorarrow", title: "Accessibility", detail: "So agents can click, type and read buttons", on: asker.accessibility)
            }
            .padding(.top, 4)
            if let next = model.nextPermission {
                Button("Allow \(next)") { model.openSettings(next) }.atlas(large: true)
                    .keyboardShortcut(.defaultAction)
                    .padding(.top, 6)
            }
            Note(text: "Atlas notices each switch on its own. If macOS offers to quit and reopen Atlas, either choice is fine.")
        }
    }
}

struct PermissionRow: View {
    let symbol: String
    let title: String
    let detail: String
    let on: Bool
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 14))
                .frame(width: 30, height: 30)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color.white.opacity(0.08)))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13, weight: .semibold))
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
    @ObservedObject var model: OnboardingModel
    let pane: String
    var body: some View {
        let on = model.granted(pane)
        VStack(spacing: 12) {
            Eyebrow(text: pane == "Accessibility" ? "1 of 2 · Accessibility" : "2 of 2 · Screen Recording")
            Headline(title: "Turn on Atlas.", size: 22).padding(.bottom, 4)
            StepRow(index: 1, title: "Find Atlas in the list", detail: "Privacy & Security › \(pane)", state: .done)
            StepRow(index: 2, title: "Turn its switch on", detail: "Confirm with Touch ID or your password", state: on ? .done : .current)
            StepRow(index: 3, title: "Come back here", detail: "Atlas notices on its own", state: on ? .done : .waiting)
            Spacer(minLength: 8)
            StatusPill(text: on ? "\(pane) is on" : "Waiting for \(pane)…", done: on)
            Button("Back") { model.guiding = nil }.atlas(.plain)
        }
        .padding(.horizontal, 26)
        .padding(.top, 44)
        .padding(.bottom, 18)
    }
}

struct HelperDoneScreen: View {
    @ObservedObject var model: OnboardingModel
    var body: some View {
        let asker = model.facts.asker
        Column(width: 450) {
            Check(size: 64)
            Headline(
                title: "All set here.",
                tail: asker.name.isEmpty ? "Switch back to your own account." : "Switch back to \(asker.title).")
            .padding(.top, 6)
            Lede(text: "Atlas starts in this account on its own from now on. You won't need to come back; Atlas finishes in your own account.")
            Button("Switch Back") { model.switchBack() }.atlas(large: true)
                .keyboardShortcut(.defaultAction)
                .disabled(model.working != nil)
                .padding(.top, 10)
            Working(model: model)
            Note(text: "Locks this account and opens the lock screen.")
        }
    }
}
