use strict;
use warnings;
use Errno qw(EINTR);

my ($events_path, $leader_path, $descendant_path, $mode) = @ARGV;
open(my $events, '+<', $events_path) or die $!;
open(my $leader, '+<', $leader_path) or die $!;
open(my $descendant, '+<', $descendant_path) or die $!;
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

$SIG{TERM} = sub { report('term'); exit 0; };
$SIG{TERM} = sub { report('term'); } if $mode eq 'stubborn';
$SIG{TERM} = 'DEFAULT' if $mode eq 'term';
report('ready') unless $mode eq 'unready';
permit($leader, 'start');
if ($mode eq 'flood') {
    print "y\n" x (1024 * 1024);
    report('output');
    while (1) { print "y\n" x 32768; }
}
if ($mode eq 'group' || $mode eq 'pipe' || $mode eq 'term' || $mode eq 'stubborn') {
    my $child = fork();
    die $! unless defined $child;
    if (!$child) {
        $SIG{TERM} = $mode eq 'term' ? 'DEFAULT' : sub { report('term'); };
        report('descendant');
        permit($descendant, 'close');
        close STDOUT;
        close STDERR;
        report('closed');
        permit($descendant, 'exit');
        exit 0;
    }
    permit($leader, 'exit');
    report('exiting');
    exit 0;
}
if ($mode eq 'output') {
    permit($leader, 'output');
    print 'one';
    report('output');
    permit($leader, 'exit');
    print 'two';
    report('exiting');
    exit 0;
}
permit($leader, 'exit');
