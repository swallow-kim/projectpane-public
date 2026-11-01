import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import { loadStartupConfig } from './shared/startup';
import './style.css';
import './ui.css';

void loadStartupConfig().then(startup => {
  const root = document.getElementById('root');
  if (root) createRoot(root).render(<App startup={startup} />);
});
