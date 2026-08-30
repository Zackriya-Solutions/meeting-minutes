declare module "bun:test" {
	export const describe: (...args: any[]) => any;
	export const test: (...args: any[]) => any;
	export const expect: any;
	export const mock: (...args: any[]) => any;
	export const beforeEach: (...args: any[]) => any;
	export const afterEach: (...args: any[]) => any;
}
