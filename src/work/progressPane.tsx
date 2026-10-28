import { duration, mark, markText, remainingText, stepFraction, stepRemaining, workflowPhases } from "./format";
import { NameForm } from "./names";
import type { NameFix, NamesPending, StepView } from "./types";

export function ProgressPane({
  downloading,
  download,
  running,
  error,
  liveFraction,
  liveDetail,
  elapsed,
  remaining,
  fraction,
  started,
  queueCount,
  steps,
  now,
  canRetry,
  nameReview,
  nameRows,
  nameNote,
  nameSending,
  onRetry,
  onNames,
  onSubmit,
  onStop,
  onSkipCurrent,
  onRunQueue,
  onNew,
}: {
  downloading: boolean;
  download: { detail: string; progress: number | null };
  running: boolean;
  error: string;
  liveFraction: number | null;
  liveDetail: string;
  elapsed: number;
  remaining: number | null;
  fraction: number;
  started: boolean;
  queueCount: number;
  steps: StepView[];
  now: number;
  canRetry: boolean;
  nameReview: NamesPending | null;
  nameRows: NameFix[];
  nameNote: string;
  nameSending: boolean;
  onRetry: () => void;
  onNames: (rows: NameFix[]) => void;
  onSubmit: (keep: boolean) => void;
  onStop: () => void;
  onSkipCurrent: () => void;
  onRunQueue: () => void;
  onNew: () => void;
}) {
  const activeStep = steps.find((step) => step.state === "running");
  const ran = [`đã chạy ${duration(elapsed)}`, remainingText(remaining), queueCount > 0 ? `còn ${queueCount} trong hàng` : ""]
    .filter(Boolean)
    .join(" · ");
  return (
    <div className="pane-fill">
      {running && queueCount>0 ? <button type="button" className="link" onClick={onSkipCurrent}>Bỏ qua video này, chạy video kế</button> : null}
      <div className="overall" aria-live="polite">
        <div className="overall-row">
          <strong>{nameReview ? "Đang chờ bạn kiểm tra tên" : liveFraction !== null ? `${Math.round(liveFraction * 100)}%` : liveDetail}</strong>
          <span>
            {downloading
              ? download.detail
              : running
                ? nameReview ? "Thời gian chờ không tính vào thời gian xử lý" : ran
                : error
                  ? ""
                  : started
                    ? `chạy ${duration(elapsed)}`
                    : ""}
          </span>
        </div>
        {downloading ? (
          <div
            className={download.progress === null ? "bar is-wait" : "bar"}
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={download.progress === null ? undefined : Math.round(download.progress * 100)}
          >
            <i style={{ width: download.progress === null ? "100%" : `${download.progress * 100}%` }} />
          </div>
        ) : (
          <div className="bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(fraction * 100)}>
            <i style={{ width: `${fraction * 100}%` }} />
          </div>
        )}
      </div>
      <ol className="workflow-phases" aria-label="Các giai đoạn xử lý">
        {workflowPhases(steps, nameReview !== null).map((phase, index) => (
          <li key={phase.id} data-state={phase.state} aria-current={phase.state === "running" || phase.state === "waiting" ? "step" : undefined}>
            <span>{phase.state === "done" ? "✓" : index + 1}</span>
            <strong>{phase.label}</strong>
            <small>{phase.state === "waiting" ? "Chờ bạn" : markText(phase.state)}</small>
          </li>
        ))}
      </ol>
      {!nameReview && activeStep ? <p className="phase-current" aria-live="polite"><strong>{activeStep.label}</strong>{activeStep.detail ? ` · ${activeStep.detail}` : ""}</p> : null}
      {error ? (
        <p className="error">
          {error}{" "}
          {!running && canRetry ? (
            <button type="button" className="link" onClick={() => void onRetry()}>
              Thử lại
            </button>
          ) : null}
        </p>
      ) : null}
      {nameReview ? (
        <NameForm
          title="Kiểm tra tên trước khi dịch"
          intro="AI điền sẵn tên bị chép sai. Sửa hoặc thêm, rồi dịch tiếp. Phụ đề và giọng đều dùng tên đúng."
          note={nameReview.note}
          error={nameNote}
          rows={nameRows}
          disabled={nameSending}
          onChange={onNames}
          primary="Dịch tiếp"
          secondary="Giữ nguyên"
          onPrimary={() => void onSubmit(false)}
          onSecondary={() => void onSubmit(true)}
          onStop={() => void onStop()}
        />
      ) : (
        <>
          <div className="pane-scroll">
            <details className="progress-details" open={error ? true : undefined}>
              <summary>Chi tiết {steps.length} bước</summary>
            <ol className="steps">
              {steps.map((step, index) => {
                const percent = Math.round(stepFraction(step) * 100);
                const left = remainingText(stepRemaining(step, now));
                const measured = step.state === "done" || (step.state === "running" && step.progress !== null);
                return (
                  <li key={step.id} className="step">
                    <span className="index">{String(index + 1).padStart(2, "0")}</span>
                    <span>
                      <strong>{step.label}</strong>
                      {step.detail ? <small>{step.detail}</small> : null}
                      {step.state === "running" || step.state === "done" ? (
                        <span className={measured ? "bar bar-sm" : "bar bar-sm is-wait"}>
                          <i style={{ width: measured ? `${percent}%` : "100%" }} />
                        </span>
                      ) : null}
                    </span>
                    <span className="step-side">
                      <span className={mark(step.state)}>
                        {step.state === "running" && step.progress !== null ? `${percent}%` : markText(step.state)}
                      </span>
                      {left ? <small>{left}</small> : null}
                    </span>
                  </li>
                );
              })}
            </ol>
            </details>
          </div>
          <footer className="pane-foot">
            {running ? (
              <button type="button" className="check" onClick={() => void onStop()}>
                Dừng
              </button>
            ) : (
              <span className="foot-actions">
                {queueCount > 0 ? (
                  <button type="button" className="check" onClick={() => void onRunQueue()}>
                    Chạy tiếp ({queueCount})
                  </button>
                ) : null}
                <button type="button" className="link" onClick={onNew}>
                  Video mới
                </button>
              </span>
            )}
          </footer>
        </>
      )}
    </div>
  );
}
