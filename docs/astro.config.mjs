// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://shaogme.github.io',
	base: '/inplace-init',
	integrations: [
		starlight({
			title: 'inplace-init',
			description: 'In-place and pinned initialization for Rust.',
			defaultLocale: 'root',
			locales: {
				root: { label: 'English', lang: 'en' },
				'zh-cn': { label: '简体中文', lang: 'zh-CN' },
			},
			sidebar: [
				{
					label: 'Start here',
					translations: { 'zh-CN': '开始使用' },
					items: [
						{ slug: 'index', label: 'Overview', translations: { 'zh-CN': '概览' } },
						{ slug: 'guide/getting-started', label: 'Getting started', translations: { 'zh-CN': '快速入门' } },
						{ slug: 'concepts/initialization-model', label: 'Initialization model', translations: { 'zh-CN': '初始化模型' } },
					],
				},
				{
					label: 'Guides',
					translations: { 'zh-CN': '使用指南' },
					items: [
						{ slug: 'guides/derive-builder', label: 'Derive and builder', translations: { 'zh-CN': 'derive 与 builder' } },
						{ slug: 'guides/pinning', label: 'Pinned initialization', translations: { 'zh-CN': '固定地址初始化' } },
						{ slug: 'guides/defaults', label: 'Default values', translations: { 'zh-CN': '默认值' } },
						{ slug: 'guides/errors-and-safety', label: 'Errors and safety', translations: { 'zh-CN': '错误处理与安全' } },
					],
				},
				{
					label: 'Reference',
					translations: { 'zh-CN': '参考' },
					items: [
						{ slug: 'reference/macros', label: 'Macros', translations: { 'zh-CN': '宏' } },
						{ slug: 'reference/api', label: 'Runtime API', translations: { 'zh-CN': '运行时 API' } },
					],
				},
			],
		}),
	],
});
