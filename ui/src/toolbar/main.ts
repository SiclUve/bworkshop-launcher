import '../shared/fonts.css';
import '../shared/theme.css';

import { mount } from 'svelte';
import Toolbar from './Toolbar.svelte';

const target = document.getElementById('app');
if (target) mount(Toolbar, { target });
