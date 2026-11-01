use strict;
use warnings;
use Errno qw(EINTR);
use POSIX ();

my ($events_path, $leader_path, $descendant_path, $parent_path, $mode) = @ARGV;
open(my $events, '+<', $events_path) or die $!;
open(my $leader, '+<', $leader_path) or die $!;
open(my $descendant, '+<', $descendant_path) or die $!;
open(my $parent, '+<', $parent_path) or die $!;
select((select($events), $| = 1)[0]);
$| = 1;

sub report { print $events "$_[0] $$\n" or die $!; }
sub permit {
    my ($pipe, $expected) = @_;
    my $line = '';
    while ($line !~ /\n$/) {
        my $count = sysread($pipe, my $byte, 1);
        next if !defined($count) && $! == EINTR;
        die 'lost control pipe' unless $count;
        $line .= $byte;
    }
    die "unexpected permit: $line" unless $line eq "$expected\n";
}

$SIG{TERM} = $mode eq 'ignore' ? sub { report('term'); } : sub { report('term'); exit 0; };
report('ready');
permit($leader, 'start');
if ($mode eq 'escape') {
    my $member = fork();
    die $! unless defined $member;
    if (!$member) {
        report('member');
        while (1) { POSIX::pause(); }
    }
    permit($leader, 'fork');
    my $child = fork();
    die $! unless defined $child;
    if (!$child) {
        report('parent');
        permit($parent, 'fork');
        my $escaped = fork();
        die $! unless defined $escaped;
        if (!$escaped) {
            POSIX::setsid() >= 0 or die $!;
            report('escaped');
            permit($descendant, 'work');
            exec '/bin/sleep', '61';
            die $!;
        }
        permit($parent, 'exit');
        exit 0;
    }
    waitpid($child, 0) == $child or die $!;
    report('parent-reaped');
    permit($leader, 'exit');
    exit 0;
}
if ($mode eq 'group') {
    my $child = fork();
    die $! unless defined $child;
    if (!$child) {
        report('descendant');
        permit($descendant, 'exit');
        exit 0;
    }
    permit($leader, 'exit');
    exit 3;
}
if ($mode eq 'log') {
    print "\0" x 5000;
    report('output');
}
permit($leader, 'exit');
