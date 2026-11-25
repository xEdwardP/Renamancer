import { ChevronDown } from 'lucide-react';
import type { FieldError, Mode, Rule } from '../../types';

const modes: { value: Mode; label: string }[] = [
  { value: 'episode', label: 'Episodio' },
  { value: 'regex', label: 'Regex' },
  { value: 'numbering', label: 'Numerar' },
  { value: 'text', label: 'Texto' },
];

type Props = {
  rule: Rule;
  errors: FieldError[];
  advanced: boolean;
  onAdvanced(visible: boolean): void;
  onMode(mode: Mode): void;
  onEdit(rule: Rule): void;
};

export function RulePanel({ rule, errors, advanced, onAdvanced, onMode, onEdit }: Props) {
  function field(key: string, label: string, value: string, change: (value: string) => void, help?: string) {
    const error = errors.find(({ field }) => field === key)?.message;
    const numeric = ['season', 'digits', 'start', 'step', 'width'].includes(key);
    return (
      <div className="field">
        <label htmlFor={key}>{label}</label>
        <input id={key} value={value} spellCheck={false} autoComplete="off"
          inputMode={numeric ? 'numeric' : undefined} aria-invalid={!!error}
          aria-describedby={error || help ? key + '-hint' : undefined}
          onChange={(event) => change(event.target.value)} />
        {(error || help) && <small id={key + '-hint'} className={error ? 'error' : 'muted'}>{error || help}</small>}
      </div>
    );
  }

  return (
    <aside className="panel rule" aria-labelledby="rule-title">
      <h2 id="rule-title">Regla</h2>
      <div className="mode-switch" role="group" aria-label="Modo de renombrado">
        {modes.map(({ value, label }) => (
          <button key={value} type="button" aria-pressed={rule.mode === value} onClick={() => onMode(value)}>{label}</button>
        ))}
      </div>
      <div className="rule-fields">
        {rule.mode === 'episode' && (
          <>
            <div className="rule-numbers">
              {field('season', 'Temporada', rule.season, (season) => onEdit({ ...rule, season }))}
              {field('digits', 'Dígitos', rule.digits, (digits) => onEdit({ ...rule, digits }))}
            </div>
            <button className="advanced-toggle" aria-expanded={advanced} aria-controls="advanced" onClick={() => onAdvanced(!advanced)}>
              Opciones avanzadas <ChevronDown size={17} aria-hidden="true" />
            </button>
            <div id="advanced" className="advanced-fields" hidden={!advanced}>
              {field('pattern', 'Patrón del nombre actual', rule.pattern, (pattern) => onEdit({ ...rule, pattern }),
                'El primer grupo captura el número de episodio.')}
              {field('template', 'Plantilla del nombre nuevo', rule.template, (template) => onEdit({ ...rule, template }),
                'Variables: {nombre}, {temporada}, {episodio}, {fecha} y {ext}.')}
            </div>
          </>
        )}
        {rule.mode === 'regex' && (
          <>
            {field('pattern', 'Patrón', rule.pattern, (pattern) => onEdit({ ...rule, pattern }))}
            {field('replacement', 'Reemplazo', rule.replacement, (replacement) => onEdit({ ...rule, replacement }),
              'Usa $1 o ${nombre} para capturas y $$ para un dólar literal. Una captura ausente aporta texto vacío.')}
            <label className="inline-option">
              <input type="checkbox" checked={rule.ignoreCase}
                onChange={(event) => onEdit({ ...rule, ignoreCase: event.target.checked })} />
              Ignorar mayúsculas
            </label>
          </>
        )}
        {rule.mode === 'numbering' && (
          <>
            <div className="rule-numbers">
              {field('start', 'Inicio', rule.start, (start) => onEdit({ ...rule, start }))}
              {field('step', 'Paso', rule.step, (step) => onEdit({ ...rule, step }))}
            </div>
            {field('width', 'Relleno', rule.width, (width) => onEdit({ ...rule, width }))}
            {field('template', 'Plantilla', rule.template, (template) => onEdit({ ...rule, template }),
              'Incluye {n} o un ancho como {n:03}. También puedes usar {nombre}, {fecha} y {ext}.')}
          </>
        )}
        {rule.mode === 'text' && (
          <>
            {field('search', 'Buscar', rule.search, (search) => onEdit({ ...rule, search }))}
            {field('replacement', 'Reemplazo', rule.replacement, (replacement) => onEdit({ ...rule, replacement }))}
            <div className="rule-numbers">
              {field('prefix', 'Prefijo', rule.prefix, (prefix) => onEdit({ ...rule, prefix }))}
              {field('suffix', 'Sufijo', rule.suffix, (suffix) => onEdit({ ...rule, suffix }))}
            </div>
            <div className="field">
              <label htmlFor="capitalization">Mayúsculas y minúsculas</label>
              <select id="capitalization" value={rule.capitalization} onChange={(event) => {
                const value = event.target.value;
                if (value === 'keep' || value === 'upper' || value === 'lower') onEdit({ ...rule, capitalization: value });
              }}>
                <option value="keep">Conservar</option>
                <option value="upper">Mayúsculas</option>
                <option value="lower">Minúsculas</option>
              </select>
            </div>
          </>
        )}
        <div className="field extension-option">
          <label htmlFor="extension-kind">Extensión</label>
          <select id="extension-kind" value={rule.extension.kind} onChange={(event) => {
            const kind = event.target.value;
            if (kind === 'change') onEdit({ ...rule, extension: { kind, value: '' } });
            if (kind === 'keep' || kind === 'remove') onEdit({ ...rule, extension: { kind } });
          }}>
            <option value="keep">Conservar</option>
            <option value="change">Cambiar</option>
            <option value="remove">Quitar</option>
          </select>
        </div>
        {rule.extension.kind === 'change' && field('extension', 'Extensión nueva', rule.extension.value,
          (value) => onEdit({ ...rule, extension: { kind: 'change', value } }), 'Por ejemplo, mkv o .mkv.')}
      </div>
    </aside>
  );
}
