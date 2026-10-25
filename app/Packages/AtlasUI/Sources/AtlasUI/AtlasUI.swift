/// AtlasUI — level 0, its own package (`app/Packages/AtlasUI`), which Atlas.app's package depends on by path.
///
/// Owns: the design system (`DesignSystem/`: colours, type, spacing, radii, strokes, sizes, opacities, motion), the
/// reusable views every window composes (`Components/`), each window's state and events (`Model/`), the windows'
/// views (`Panel/`, `Viewer/`, `Onboarding/`, `Settings/`, `Alert/`), and the samples every preview and design
/// render draws (`PreviewStates/`, debug builds only).
/// Never imports: a domain package, AppKit or the app. A view is a function of its state struct and sends events; it
/// decides nothing. Its words follow one rule: fixed copy (a title, a button, a step or a check that reads the same in
/// every state) stays in the view, keyed by its case where a state picks among them; every word that depends on
/// state, interpolated, chosen by a condition or derived from facts, arrives final from the app's presenters, and a
/// list row's fixed name travels with its value.
/// Seams: none. The app passes each window's state in, takes its events, fills the viewer's input slot with its own
/// AppKit view, and hands the viewer the status line's detail to read as it draws. Only what the app reaches is
/// `public`: each window's root view, its state and events, and the few tokens its AppKit shells share.
