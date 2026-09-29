# Fixture for milestone 1034 (#971 part A), User Story 3.
#
# Evaluating this flake WILL build a derivation and run the `/bin/sh` command
# below — unless import-from-derivation is refused. That is the measured
# behaviour of `nix eval` with no `--impure` and no extra options (research
# R3), and it is the reason the tier is opt-in and gates on a verified refusal.
#
# The marker is deliberately harmless: it writes a string to $out. A scan of
# this fixture must leave no `waybill-ifd-marker` path in the Nix store.
{
  outputs = _: {
    packages.x86_64-linux.default = import (derivation {
      name = "waybill-ifd-marker";
      system = "x86_64-linux";
      builder = "/bin/sh";
      args = [ "-c" "echo '\"ifd-ran\"' > $out" ];
    });
    packages.aarch64-darwin.default = import (derivation {
      name = "waybill-ifd-marker";
      system = "aarch64-darwin";
      builder = "/bin/sh";
      args = [ "-c" "echo '\"ifd-ran\"' > $out" ];
    });
  };
}
