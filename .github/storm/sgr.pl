#!/usr/bin/env perl
# Rewrite each SGR sequence in the ANSI text on stdin, or in the files named,
# so that it resets everything and then sets the attributes still on.
# Usage: .github/storm/sgr.pl [file...]
#
# freeze 0.2.2 ignores the SGR codes that reset one attribute, such as 39 and
# 49 for the default colors, so a color runs on past its field. shot.sh
# renders from this output. Underline color (58 and 59) is dropped, since
# freeze does not draw it; a sequence with ':' passes through unchanged.
use strict;
use warnings;

my %on;

sub sgr {
  my ($params) = @_;
  return "\e[${params}m" if $params =~ /:/;
  my @p = split /;/, $params, -1;
  @p = (0) unless @p;
  while (@p) {
    my $c = shift(@p) || 0;
    if ($c == 0) { %on = () }
    elsif ($c >= 1 && $c <= 9) { $on{$c} = $c }
    elsif ($c == 22) { delete @on{1, 2} }
    elsif ($c >= 23 && $c <= 29) { delete $on{$c - 20} }
    elsif (($c >= 30 && $c <= 37) || ($c >= 90 && $c <= 97)) { $on{fg} = $c }
    elsif ($c == 39) { delete $on{fg} }
    elsif (($c >= 40 && $c <= 47) || ($c >= 100 && $c <= 107)) { $on{bg} = $c }
    elsif ($c == 49) { delete $on{bg} }
    elsif ($c == 38 || $c == 48 || $c == 58) {
      my $mode = shift(@p) || 0;
      my @args = $mode == 5 ? (shift(@p) || 0) : $mode == 2 ? (map { shift(@p) || 0 } 1 .. 3) : ();
      next if $c == 58 || !@args;
      $on{$c == 38 ? "fg" : "bg"} = join ";", $c, $mode, @args;
    }
  }
  return "\e[" . join(";", 0, grep { defined } @on{1 .. 9, "fg", "bg"}) . "m";
}

while (my $line = <>) {
  $line =~ s/\e\[([0-9;:]*)m/sgr($1)/ge;
  print $line;
}
