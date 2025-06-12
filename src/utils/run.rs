use crate::utils::selinux_enabled;
use anyhow::{anyhow, Context, Result};
use bollard::{
    container::LogOutput,
    query_parameters::{
        AttachContainerOptionsBuilder, CreateContainerOptionsBuilder, CreateImageOptionsBuilder,
        RemoveContainerOptionsBuilder, StartContainerOptionsBuilder, StopContainerOptionsBuilder,
        WaitContainerOptionsBuilder,
    },
    secret::{ContainerCreateBody, CreateImageInfo, HostConfig},
    Docker,
};
use futures::{FutureExt, StreamExt, TryStreamExt};
use rand::{
    distr::{Alphanumeric, SampleString},
    rng,
};
use std::{
    io::{stderr, stdout, Write},
    path::Path,
};
use tokio::{select, signal};

pub async fn run_init<T: AsRef<Path>>(path: T, file_path: T) -> Result<()> {
    let path = path.as_ref().to_str().context("path was not kosher")?;
    let init_file = file_path.as_ref().to_str().context("path was not kosher")?;
    let image = "registry.opensuse.org/opensuse/tumbleweed:latest";

    let api = Docker::connect_with_defaults().context("Failed to contect to podman api")?;

    let ctrl_c = signal::ctrl_c().fuse();

    let create_image_options = CreateImageOptionsBuilder::new().from_image(image).build();

    let mut stream = api
        .create_image(Some(create_image_options), None, None)
        .fuse();

    select! {
        maybe_output = stream.try_next() => {
                match maybe_output {
                    Ok(Some(CreateImageInfo {
                        status: Some(status),
                        progress: Some(progress),
                        id,
                        ..
                    })) => {
                        if let Some(id) = id {
                            println!("{:>20}: {:<15} {}", id, status, progress);
                        } else {
                            println!("{:<15} {}", status, progress);
                        }
                    },
                    Ok(Some(CreateImageInfo {
                        status: Some(status),
                        id,
                        ..
                    })) => {
                        if let Some(id) = id {
                            println!("{:>20}: {}", id, status);
                        } else {
                            println!("{}", status);
                        }
                    },
                    Ok(Some(CreateImageInfo {status: None, ..})) => { println!("how did you get here")},
                    Ok(None) => {
                        println!("\nPull complete.");
                    },
                    Err(e) => {
                        return Err(anyhow!("Error pulling image: {}", e));
                    }
                }
            }

        _ = ctrl_c => {
            println!("\nPull canceled by user.");

            return Err(anyhow!("Interrupted by Ctrl+C"));
        }
    }

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
        image: Some(image.into()),
        host_config: Some(host_config),
        env: Some(vec![
            "ZYPP_PCK_PRELOAD=1".to_string(),
            "ZYPP_CURL2=1".to_string(),
            "ZYPP_SINGLE_RPMTRANS=1".to_string(),
        ]),
        ..Default::default()
    };

    let short_rng = Alphanumeric.sample_string(&mut rng(), 8);
    let container_name = format!("build-container-{short_rng}");

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
                LogOutput::StdOut { message } => {
                    let _ = stdout().write_all(&message);
                    let _ = stdout().flush();
                }
                LogOutput::StdErr { message } => {
                    let _ = stderr().write_all(&message);
                    let _ = stderr().flush();
                }
                LogOutput::StdIn { .. } => {}
                LogOutput::Console { message } => {
                    let _ = stdout().write_all(&message);
                    let _ = stdout().flush();
                }
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

        _ = signal::ctrl_c() => {
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
