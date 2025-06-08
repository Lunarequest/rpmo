use crate::utils::selinux_enabled;
use anyhow::{anyhow, Context, Result};
use bollard::{
    container::LogOutput,
    query_parameters::{
        AttachContainerOptionsBuilder, CreateContainerOptionsBuilder,
        RemoveContainerOptionsBuilder, StartContainerOptionsBuilder, StopContainerOptionsBuilder,
        WaitContainerOptionsBuilder,
    },
    secret::{ContainerCreateBody, HostConfig},
    Docker, API_DEFAULT_VERSION,
};
use futures::StreamExt;
use rand::{
    distr::{Alphanumeric, SampleString},
    rng,
};
use std::path::Path;
use tokio::{select, signal::ctrl_c};

pub async fn run_init<T: AsRef<Path>>(path: T, file_path: T) -> Result<()> {
    let path = path.as_ref().to_str().context("path was not kosher")?;
    let init_file = file_path.as_ref().to_str().context("path was not kosher")?;

    let mut host_config = HostConfig {
        binds: Some(vec![
            format!("{path}:/newroot"),
            format!("{init_file}:/init.sh"),
        ]),
        cap_add: Some(vec!["CAP_SYS_CHROOT".into(), "SYS_ADMIN".into()]),
        ..Default::default()
    };

    let mut security_opt = vec!["unmask=/proc/*".to_string()];
    if selinux_enabled() {
        security_opt.push("label=disable".into());
    }
    host_config.security_opt = Some(security_opt);

    let container_body = ContainerCreateBody {
        attach_stdin: Some(false),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(true),
        cmd: Some(vec!["/bin/bash".into(), "-x".into(), "/init.sh".into()]),
        image: Some("registry.opensuse.org/opensuse/tumbleweed:latest".into()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let short_rng = Alphanumeric.sample_string(&mut rng(), 8);
    let container_name = format!("build-container-{short_rng}");

    let api = Docker::connect_with_unix(
        "/run/user/1000/podman/podman.sock",
        120,
        API_DEFAULT_VERSION,
    )
    .context("Failed to contect to podman api")?;

    api.create_container(
        Some(
            CreateContainerOptionsBuilder::new()
                .name(&container_name)
                .build(),
        ),
        container_body,
    )
    .await
    .context("Failed to create build container")?;

    let options = StartContainerOptionsBuilder::new().build();
    let attach_container = AttachContainerOptionsBuilder::new()
        .stdout(true)
        .stderr(true)
        .stdin(false)
        .stream(true)
        .logs(false)
        .build();
    api.start_container(&container_name, Some(options))
        .await
        .context("Failed to start container")?;

    let mut output_stream = api
        .attach_container(&container_name, Some(attach_container))
        .await
        .context("Failed to attach to container output")?;

    tokio::spawn(async move {
        while let Some(Ok(output)) = output_stream.output.next().await {
            match output {
                LogOutput::StdOut { message } => print!("{}", String::from_utf8_lossy(&message)),
                LogOutput::StdErr { message } => eprint!("{}", String::from_utf8_lossy(&message)),
                LogOutput::StdIn { .. } => {}
                LogOutput::Console { message } => print!("{}", String::from_utf8_lossy(&message)),
            }
        }
    });

    let waitopts = WaitContainerOptionsBuilder::new()
        .condition("next-exit")
        .build();
    let mut wait = api.wait_container(&container_name, Some(waitopts));

    select! {
        _ = async {
            if let Some(res) = wait.next().await {
                let result = res.context("Failed to wait for container")?;
                if let Some(error) = result.error {
                    return Err(anyhow!(
                        "Container error: {}",
                        error.message.unwrap_or_else(|| "Unknown error".to_string())
                    ));
                }

                if result.status_code != 0 {
                    return Err(anyhow!(
                        "Container exited with non-zero status: {}",
                        result.status_code
                    ));
                }

            }
            Ok(())
        } => {
            // Container exited normally
        }

        _ = ctrl_c() => {
            eprintln!("\nCtrl+C received. Stopping container...");
            let stop_container_opts = StopContainerOptionsBuilder::new().signal("SIGKILL").build();
            api
                .stop_container(&container_name, Some(stop_container_opts))
                .await
                .ok(); // Ignore error here

            return Err(anyhow!("Interrupted by Ctrl+C"));
        }
    }
    let remove_options = RemoveContainerOptionsBuilder::new().force(true).build();
    api.remove_container(&container_name, Some(remove_options))
        .await
        .context("Failed to remove container")?;

    Ok(())
}
