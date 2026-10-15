import AppKit
import AtlasKit
import SwiftUI

/// The window's content: one screen at a time, chosen by the model. The
/// screens themselves are in ControllerScreens.swift (the person's account)
/// and HelperScreens.swift (the helper's).
struct OnboardingView: View {
    let model: OnboardingModel

    var body: some View {
        ZStack {
            AtlasStyle.ground.ignoresSafeArea()
            if let permission = model.guiding {
                GuidePanel(model: model, permission: permission)
                    .transition(.opacity)
            } else if !model.loaded {
                VStack(spacing: 12) {
                    Spinner(size: 18)
                    if let problem = model.problem { Note(text: problem, color: AtlasStyle.orange) }
                }
            } else {
                Screen(model: model)
                    .id(model.step)
                    .transition(
                        .asymmetric(
                            insertion: .opacity.combined(with: .offset(y: 14)).animation(
                                .easeOut(duration: 0.4).delay(0.08)),
                            removal: .opacity.combined(with: .offset(y: -10)).animation(.easeOut(duration: 0.2))))
            }
        }
        .foregroundStyle(AtlasStyle.ink)
        .environment(\.colorScheme, .dark)
    }
}

/// One full screen: its content above, its footer below.
struct Screen: View {
    let model: OnboardingModel

    var body: some View {
        VStack(spacing: 0) {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.top, 36)
            footer
        }
    }

    @ViewBuilder private var content: some View {
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

    private var footer: some View {
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

    @ViewBuilder private var back: some View {
        switch model.step {
        case .chooseHelper where !model.facts.configured:
            Button("Back") { model.choices.started = false }.atlas(.plain)
                .disabled(model.working != nil)
        case .createAccount:
            Button("Back") { model.choices.creating = false }.atlas(.plain)
                .disabled(model.working != nil)
        case .done:
            Button("Back") { model.choices.agentsDone = false }.atlas(.plain)
                .disabled(model.working != nil)
        default: EmptyView()
        }
    }

    @ViewBuilder private var next: some View {
        switch model.step {
        case .chooseHelper:
            Button("Continue") { Task { await model.useHelper() } }.atlas()
                .disabled(model.picked == nil || model.working != nil)
                .keyboardShortcut(.defaultAction)
        case .createAccount:
            Button("Create Account") { Task { await model.createAccount() } }.atlas()
                .disabled(!model.canCreate || model.working != nil)
                .keyboardShortcut(.defaultAction)
        case .finishing:
            Button("Continue") { model.refresh() }.atlas()
                .disabled(model.finish != .passed)
        case .agents:
            Button("Continue") { Task { await model.connectAgents() } }.atlas()
                .disabled(model.working != nil || !model.agentsLoaded)
                .keyboardShortcut(.defaultAction)
        case .done:
            Button("Done") { Task { await model.complete() } }.atlas()
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
    let model: OnboardingModel

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
