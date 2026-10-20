import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App';
import { initTheme } from './theme';
import { initThemeColors } from './hooks/useThemeColors';
import { initGlass } from './hooks/useGlass';

// Restore the saved theme, its colour overrides, and the glass look before first
// paint so none of them flash.
initTheme();
initThemeColors();
initGlass();

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>);
