import { createRoot } from 'react-dom/client';
import { ControlPanel } from './controls/control-panel';
import { Desktop } from './desktop/desktop';
import './sandbox.css';

function SandboxApp() {
  return (
    <div className='sandbox-root'>
      <div className='sandbox-stage'>
        <Desktop />
      </div>
      <ControlPanel />
    </div>
  );
}

createRoot(document.getElementById('sandbox-root')!).render(<SandboxApp />);
