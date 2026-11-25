import '../shared/fonts.css';
import '../shared/theme.css';

import { mount } from 'svelte';
import WidgetBar from './WidgetBar.svelte';

const target = document.getElementById('app');
if (target) mount(WidgetBar, { target });
