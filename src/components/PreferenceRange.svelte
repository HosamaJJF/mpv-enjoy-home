<script lang="ts">
  let {
    label,
    description,
    overridden,
    draft,
    min,
    max,
    step,
    output,
    disabled = false,
    onToggle,
    onDraft,
    onCommit,
  }: {
    label: string;
    description: string;
    overridden: boolean;
    draft: number;
    min: number;
    max: number;
    step: number;
    output: string;
    disabled?: boolean;
    onToggle: (enabled: boolean) => void;
    onDraft: (value: number) => void;
    onCommit: () => void;
  } = $props();
</script>

<div class="danmaku-style-row">
  <span class="preference-copy">
    <strong>{label}</strong>
    <small>{description}</small>
  </span>
  <div class="range-preference">
    <label class="override-toggle">
      <input
        type="checkbox"
        aria-label={`自定义弹幕${label}`}
        checked={overridden}
        {disabled}
        onchange={(event) => onToggle(event.currentTarget.checked)}
      />
      <span>自定义</span>
    </label>
    <input
      type="range"
      {min}
      {max}
      {step}
      value={draft}
      aria-label={`弹幕${label}`}
      disabled={!overridden || disabled}
      oninput={(event) => onDraft(event.currentTarget.valueAsNumber)}
      onchange={onCommit}
    />
    <output>{output}</output>
  </div>
</div>
