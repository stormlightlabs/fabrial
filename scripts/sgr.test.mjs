import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("../.github/storm/sgr.pl", import.meta.url));
const rewrite = input => {
  const run = spawnSync("perl", [script], { input, encoding: "utf8" });
  assert.equal(run.status, 0, run.stderr);
  return run.stdout;
};

const cases = [
  ["a reset of the foreground drops only the foreground", "\x1b[1;35mA\x1b[39m B", "\x1b[0;1;35mA\x1b[0;1m B"],
  ["a reset of the background keeps the foreground", "\x1b[31;44mA\x1b[49mB", "\x1b[0;31;44mA\x1b[0;31mB"],
  ["an empty sequence resets everything", "\x1b[4mA\x1b[mB", "\x1b[0;4mA\x1b[0mB"],
  ["22 ends bold and faint", "\x1b[1;2;3mA\x1b[22mB", "\x1b[0;1;2;3mA\x1b[0;3mB"],
  ["24 ends underline", "\x1b[4;7mA\x1b[24mB", "\x1b[0;4;7mA\x1b[0;7mB"],
  ["256 and true colors carry their arguments", "\x1b[38;5;208;48;2;1;2;3mA", "\x1b[0;38;5;208;48;2;1;2;3mA"],
  ["a 256-color underline color is dropped", "\x1b[4m\x1b[58;5;31mA\x1b[24;59m B", "\x1b[0;4m\x1b[0;4mA\x1b[0m B"],
  ["a true-color underline color is dropped", "\x1b[1m\x1b[58;2;0;0;255mA", "\x1b[0;1m\x1b[0;1mA"],
  ["59 leaves the other attributes", "\x1b[1;32m\x1b[59mA", "\x1b[0;1;32m\x1b[0;1;32mA"],
  ["25 ends slow and rapid blink", "\x1b[1;5;6mA\x1b[25mB", "\x1b[0;1;5;6mA\x1b[0;1mB"],
  ["26 changes nothing", "\x1b[6mA\x1b[26mB", "\x1b[0;6mA\x1b[0;6mB"],
  ["a curly underline is an underline", "\x1b[4:3mA\x1b[31mB", "\x1b[0;4mA\x1b[0;4;31mB"],
  ["4:0 ends underline", "\x1b[1;4mA\x1b[4:0mB", "\x1b[0;1;4mA\x1b[0;1mB"],
  ["a colon 256 color carries on", "\x1b[38:5:208mA\x1b[1mB", "\x1b[0;38;5;208mA\x1b[0;1;38;5;208mB"],
  ["a colon true color with an empty color space", "\x1b[48:2::1:2:3mA", "\x1b[0;48;2;1;2;3mA"],
  ["a colon true color with a color space", "\x1b[38:2:0:1:2:3mA", "\x1b[0;38;2;1;2;3mA"],
  ["a colon true color without a color space", "\x1b[38:2:1:2:3mA", "\x1b[0;38;2;1;2;3mA"],
  ["a colon underline color is dropped", "\x1b[4;58:2::1:2:3mA", "\x1b[0;4mA"],
  ["attributes carry across lines", "\x1b[33mA\nB\x1b[1mC\n", "\x1b[0;33mA\nB\x1b[0;1;33mC\n"],
  ["other escapes pass through", "\x1b[2J\x1b[HA", "\x1b[2J\x1b[HA"],
];

for (const [name, input, output] of cases) {
  test(`sgr.pl: ${name}`, () => assert.equal(rewrite(input), output));
}
