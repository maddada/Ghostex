use super::*;

/// CDXC:SessionTitles 2026-09-10 WHY:
/// Title workers belong to one gxserver process but their running flags survive it. Retire interrupted jobs before accepting requests, including legacy claims without attempt IDs, so no provider restores a permanent generating-title overlay. A rename may already have been submitted, so recovery must not send it again.
pub(crate) fn recover_title_jobs_after_restart(paths: &GxserverPaths) -> Result<()> {
    let db = open_gxserver_database(paths)?;
    db.execute(
        r#"
        UPDATE sessions
        SET runtimeSettingsJson = json_set(
                json_remove(runtimeSettingsJson, '$.gxserverFirstPromptAutoTitleAttemptId'),
                '$.gxserverFirstPromptAutoTitleStatus', 'failed',
                '$.gxserverFirstPromptAutoTitleReason', 'server-restarted',
                '$.gxserverFirstPromptAutoTitleFailedAt', ?1
            ),
            updatedAt = ?1
        WHERE json_extract(runtimeSettingsJson, '$.gxserverFirstPromptAutoTitleStatus') = 'running'
        "#,
        [now_iso()],
    )?;
    Ok(())
}
