use super::*;

const PRINTED: &str = "gui/501/com.insanearts.atlas.vm.x = {\n\
\tactive count = 1\n\
\tpath = /Users/p/Library/LaunchAgents/com.insanearts.atlas.vm.x.plist\n\
\tstate = running\n\
\tprogram = /Applications/Atlas.app/Contents/Library/Atlas VM.app/Contents/MacOS/AtlasVM\n\
\targuments = {\n\
\t\t/Applications/Atlas.app/Contents/Library/Atlas VM.app/Contents/MacOS/AtlasVM\n\
\t\tserve\n\
\t\tx\n\
\t}\n\
\tpid = 42\n\
\texit timeout = 30\n\
\tenvironment = {\n\
\t\tpath = /not/the/job\n\
\t}\n\
\tproperties = keepalive | inferred program\n\
}\n";

#[test]
fn a_jobs_own_fields_are_read_and_a_nested_one_is_not() {
    let job = Loaded::parse(PRINTED);

    assert_eq!(job.pid, Some(42));
    assert_eq!(
        job.path.as_deref(),
        Some(std::path::Path::new(
            "/Users/p/Library/LaunchAgents/com.insanearts.atlas.vm.x.plist"
        ))
    );
    assert_eq!(
        job.arguments.as_deref().map(<[String]>::len),
        Some(3),
        "{job:?}"
    );
    assert_eq!(job.arguments.unwrap()[1], "serve");
    assert_eq!(job.exit_timeout, Some(30));
    assert_eq!(
        job.properties,
        Some(vec!["keepalive".to_owned(), "inferred program".to_owned()])
    );
}

#[test]
fn a_field_named_twice_names_nothing() {
    let printed = format!("{PRINTED}\tpid = 7\n\tprogram = /bin/sh\n");

    let job = Loaded::parse(&printed);

    assert_eq!(job.pid, None);
    assert_eq!(job.program, None);
    assert!(job.path.is_some());
}

#[test]
fn a_field_smuggled_into_the_arguments_counts_and_leaves_them_unread() {
    let smuggled = "x = {\n\tpath = /real.plist\n\targuments = {\n\t\t/bin/prog\n\tpath = /forged.plist\n\t\tserve\n\t}\n}\n";

    let job = Loaded::parse(smuggled);

    assert_eq!(job.path, None);
    assert_eq!(job.arguments, None);
}
