use super::*;

#[test]
fn a_job_ends_when_the_screen_leaves_where_its_kind_keeps_it() {
    assert_eq!(forced(Console::Off, true, false), Some(How::OnConsole));
    assert_eq!(forced(Console::Off, false, false), None);
    assert_eq!(forced(Console::Own, false, false), Some(How::LeftScreen));
    assert_eq!(forced(Console::Own, true, false), None);
}

#[test]
fn the_persons_stop_ends_a_job_wherever_the_screen_is_kept() {
    assert_eq!(forced(Console::Off, false, true), Some(How::Person));
    assert_eq!(forced(Console::Own, true, true), Some(How::Person));
}
