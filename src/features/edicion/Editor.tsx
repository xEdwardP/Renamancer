import { AlertTriangle, ArrowRight, ChevronDown, CircleMinus, Eye, Files } from 'lucide-react';
import type { EditorScreen } from '../../store/preview';
import type { PreviewRow, Rule } from '../../types';

type Props = {
  screen: EditorScreen;
  rule: Rule;
  advanced: boolean;
  onAdvanced(visible: boolean): void;
  onEdit(field: keyof Rule, value: string): void;
};

const sizes = new Intl.NumberFormat('es', { maximumFractionDigits: 1 });

function formatSize(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const unit = bytes < 1024 ? 0 : Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${sizes.format(bytes / 1024 ** unit)} ${units[unit]}`;
}

function RowStatus({ row }: { row?: PreviewRow }) {
  if (!row) return <span className="muted">—</span>;
  if (row.status === 'proposed') return <span className="row-status proposed"><ArrowRight size={16} aria-hidden="true" />Propuesta</span>;
  if (row.status === 'unchanged') return <span className="row-status muted"><CircleMinus size={16} aria-hidden="true" />Sin cambios</span>;
  return <span className="row-status error"><AlertTriangle size={16} aria-hidden="true" /><span>Error <small>{row.reason}</small></span></span>;
}

export function Editor({ screen, rule, advanced, onAdvanced, onEdit }: Props) {
  const { selection, view } = screen;
  const rows = view.status === 'ready' ? view.rows : [];
  const errors = view.status === 'invalid' ? view.errors : [];
  const proposed = rows.filter(({ status }) => status === 'proposed').length;
  const summary = view.status === 'loading' ? 'Calculando vista previa…' :
    view.status === 'invalid' ? 'Corrige los campos de la regla para ver las propuestas.' :
    view.status === 'failed' ? view.message :
    [
      `${proposed} ${proposed === 1 ? 'propuesta' : 'propuestas'}`,
      `${rows.filter(({ status }) => status === 'unchanged').length} sin cambios`,
      `${rows.filter(({ status }) => status === 'error').length} con error`,
    ].join(' · ');

  function field(key: keyof Rule, label: string, help?: string) {
    const error = errors.find(({ field }) => field === key)?.message;
    const numeric = key === 'season' || key === 'digits';
    return (
      <div className="field">
        <label htmlFor={key}>{label}</label>
        <input id={key} value={rule[key]} spellCheck={false} autoComplete="off" inputMode={numeric ? 'numeric' : undefined}
          aria-invalid={!!error} aria-describedby={error || help ? `${key}-hint` : undefined}
          onChange={(event) => onEdit(key, event.target.value)} />
        {(error || help) && <small id={`${key}-hint`} className={error ? 'error' : 'muted'}>{error || help}</small>}
      </div>
    );
  }

  return (
    <>
      <div className="editor">
        <section className="panel preview" aria-labelledby="preview-title">
          <header className="panel-heading">
            <h1 id="preview-title"><Eye size={22} aria-hidden="true" />Vista previa</h1>
            <p>Los archivos permanecen intactos</p>
          </header>
          <div className="table-scroll" tabIndex={0} role="region" aria-label="Archivos seleccionados y nombres propuestos">
            <table>
              <thead><tr><th scope="col">Nombre actual</th><th scope="col">Tipo</th><th scope="col">Tamaño</th><th scope="col">Nombre propuesto</th><th scope="col">Estado</th></tr></thead>
              <tbody>
                {selection.files.map((file, index) => {
                  const row = rows[index];
                  return (
                    <tr key={`${selection.revision}:${index}`}>
                      <td className="mono current">{file.name}</td>
                      <td className="muted">{file.kind}</td>
                      <td className="muted size">{formatSize(file.size)}</td>
                      <td>{row?.status === 'proposed' ? <span className="candidate mono">{row.name}</span> : <span className="muted">—</span>}</td>
                      <td><RowStatus row={row} /></td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
        <aside className="panel rule" aria-labelledby="rule-title">
          <h2 id="rule-title">Regla</h2>
          <div className="mode"><Files size={18} aria-hidden="true" /> Episodio</div>
          <div className="rule-numbers">{field('season', 'Temporada')}{field('digits', 'Dígitos')}</div>
          <button className="advanced-toggle" aria-expanded={advanced} aria-controls="advanced" onClick={() => onAdvanced(!advanced)}>
            Opciones avanzadas <ChevronDown size={17} aria-hidden="true" />
          </button>
          <div id="advanced" className="advanced-fields" hidden={!advanced}>
            {field('pattern', 'Patrón del nombre actual', 'El primer grupo captura el episodio en el nombre sin extensión.')}
            {field('template', 'Plantilla del nombre nuevo', 'Variables: {nombre}, {temporada}, {episodio} y {ext}.')}
          </div>
        </aside>
      </div>
      <footer className="panel summary" role="status">{summary}</footer>
    </>
  );
}
