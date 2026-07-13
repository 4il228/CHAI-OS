use super::parser::NixPlan;

pub fn generate_nix(plan: &NixPlan) -> String {
    let mut out = String::from("{ config, pkgs, ... }: {\n");

    if !plan.packages.is_empty() {
        out.push_str("  environment.systemPackages = with pkgs; [");
        for pkg in &plan.packages {
            out.push(' ');
            out.push_str(pkg);
        }
        out.push_str(" ];\n");
    }

    for svc in &plan.services {
        let path = service_path(&svc.name);
        if svc.enabled {
            out.push_str(&format!("  {}.enable = true;\n", path));
        } else {
            out.push_str(&format!("  {}.enable = false;\n", path));
        }
    }

    out.push_str("}\n");
    out
}

fn service_path(name: &str) -> String {
    match name {
        "docker" => "virtualisation.docker".into(),
        "podman" => "virtualisation.podman".into(),
        "libvirt" => "virtualisation.libvirtd".into(),
        "sshd" | "ssh" => "services.openssh".into(),
        "nginx" => "services.nginx".into(),
        "postgresql" | "postgres" => "services.postgresql".into(),
        "mysql" | "mariadb" => "services.mysql".into(),
        "redis" => "services.redis".into(),
        "ollama" => "services.ollama".into(),
        "docker-compose" => "virtualisation.docker-compose".into(),
        other => format!("services.{}", other),
    }
}
