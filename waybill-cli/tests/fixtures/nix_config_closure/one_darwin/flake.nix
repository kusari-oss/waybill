# m1066 (#1052): a system-configuration flake with exactly one configuration
# and no `packages` output. No inputs, so evaluation needs no network; the
# derivation is never built (the closure tier only evaluates).
{
  outputs = { self }: {
    darwinConfigurations.laptop.system = derivation {
      name = "darwin-system-laptop-1.0";
      system = "aarch64-darwin";
      builder = "/bin/sh";
      args = [ "-c" "echo > $out" ];
    };
  };
}
