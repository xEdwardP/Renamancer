import { FileText } from 'lucide-react';
import type { ReactNode } from 'react';

export function Home({ select }: { select: ReactNode }) {
  return (
    <section className="panel home" aria-labelledby="home-title">
      <div className="home-icon"><FileText size={43} strokeWidth={1.7} aria-hidden="true" /></div>
      <h1 id="home-title">Elige los archivos que quieres renombrar</h1>
      <p>Selecciona archivos de una misma carpeta y revisa los nombres propuestos.</p>
      {select}
    </section>
  );
}
