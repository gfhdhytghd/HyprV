//! Narrow systemd user-manager client used at action time (unit state, jobs,
//! transient app/recorder units). The shared `systemd` actor lives in svc-sys;
//! this crate only needs a handful of calls and must not depend on it.
use crate::ServiceError;
use futures_util::StreamExt;
use std::time::Duration;
use zbus::{proxy, zvariant::OwnedObjectPath, Connection};

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Manager {
    fn subscribe(&self) -> zbus::Result<()>;
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn load_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<(String, String, String)>)>;
    fn disable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
    ) -> zbus::Result<Vec<(String, String, String)>>;
    fn set_environment(&self, assignments: &[&str]) -> zbus::Result<()>;
    #[allow(clippy::type_complexity)]
    fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: &[(&str, zbus::zvariant::Value<'_>)],
        aux: &[(&str, Vec<(&str, zbus::zvariant::Value<'_>)>)],
    ) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn job_removed(
        &self,
        id: u32,
        job: OwnedObjectPath,
        unit: String,
        result: String,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn unit_new(&self, id: String, unit: OwnedObjectPath) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Unit {
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Service",
    default_service = "org.freedesktop.systemd1"
)]
pub trait ServiceUnit {
    #[zbus(property)]
    fn control_group(&self) -> zbus::Result<String>;
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> zbus::Result<u32>;
}

#[derive(Clone)]
pub struct UserManager {
    pub conn: Connection,
    pub manager: ManagerProxy<'static>,
}
impl UserManager {
    pub async fn connect() -> Result<Self, ServiceError> {
        let conn = Connection::session().await?;
        Self::with(conn).await
    }
    pub async fn with(conn: Connection) -> Result<Self, ServiceError> {
        let manager = ManagerProxy::new(&conn).await?;
        Ok(Self { conn, manager })
    }
    /// `systemctl --user is-active --quiet UNIT`; unloaded units are inactive.
    pub async fn is_active(&self, unit: &str) -> bool {
        self.active_state(unit).await.as_deref() == Some("active")
    }
    pub async fn active_state(&self, unit: &str) -> Option<String> {
        let path = self.manager.get_unit(unit).await.ok()?;
        UnitProxy::builder(&self.conn)
            .path(path)
            .ok()?
            .build()
            .await
            .ok()?
            .active_state()
            .await
            .ok()
    }
    pub async fn control_group(&self, unit: &str) -> Option<String> {
        let path = self.manager.get_unit(unit).await.ok()?;
        ServiceUnitProxy::builder(&self.conn)
            .path(path)
            .ok()?
            .build()
            .await
            .ok()?
            .control_group()
            .await
            .ok()
    }
    /// Run a job-producing call and wait for its JobRemoved (subscribe first).
    pub async fn job<F, Fut>(&self, call: F, deadline: Duration) -> Result<String, ServiceError>
    where
        F: FnOnce(ManagerProxy<'static>) -> Fut,
        Fut: std::future::Future<Output = zbus::Result<OwnedObjectPath>>,
    {
        let _ = self.manager.subscribe().await;
        let mut removed = self.manager.receive_job_removed().await?;
        let job = call(self.manager.clone()).await?;
        tokio::time::timeout(deadline, async {
            while let Some(signal) = removed.next().await {
                if let Ok(args) = signal.args() {
                    if args.job.as_str() == job.as_str() {
                        return Ok(args.result.clone());
                    }
                }
            }
            Err(ServiceError::new("dbus", "systemd signal stream closed"))
        })
        .await
        .map_err(|_| ServiceError::new("timeout", "systemd job did not finish"))?
    }
    pub async fn start(&self, unit: &str) -> Result<(), ServiceError> {
        job_ok(
            self.job(|m| async move { m.start_unit(unit, "replace").await }, JOB)
                .await?,
        )
    }
    pub async fn stop(&self, unit: &str) -> Result<(), ServiceError> {
        job_ok(
            self.job(|m| async move { m.stop_unit(unit, "replace").await }, JOB)
                .await?,
        )
    }
    pub async fn restart(&self, unit: &str) -> Result<(), ServiceError> {
        job_ok(
            self.job(
                |m| async move { m.restart_unit(unit, "replace").await },
                JOB,
            )
            .await?,
        )
    }
    /// `systemd-run --user --collect --unit=NAME --property=Type=exec ARGV...`.
    pub async fn run_transient(
        &self,
        name: &str,
        argv: &[String],
        description: &str,
        extra: Vec<(&'static str, zbus::zvariant::Value<'static>)>,
    ) -> Result<(), ServiceError> {
        let exec = vec![(argv[0].clone(), argv.to_vec(), false)];
        let mut props: Vec<(&str, zbus::zvariant::Value<'_>)> = vec![
            ("Description", description.into()),
            ("Type", "exec".into()),
            ("CollectMode", "inactive-or-failed".into()),
            ("ExecStart", zbus::zvariant::Value::from(exec)),
        ];
        props.extend(extra);
        self.manager
            .start_transient_unit(name, "fail", &props, &[])
            .await?;
        Ok(())
    }
}
const JOB: Duration = Duration::from_secs(90);
fn job_ok(result: String) -> Result<(), ServiceError> {
    if result == "done" {
        Ok(())
    } else {
        Err(ServiceError::new("systemd", format!("job {result}")))
    }
}

/// systemd unit-name escaping for a transient app unit (`[A-Za-z0-9:_.\\-]`).
pub fn escape_unit_part(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b == b':' || b == b'_' || b == b'.' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out
}
pub fn unique_suffix() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}{:09}", now.as_secs(), now.subsec_nanos())
}
