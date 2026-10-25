// The part of @angular/core the fixture's sources use: declarations only, for ng-analyze.
export declare function Component(metadata: object): ClassDecorator;
export declare function Directive(metadata: object): ClassDecorator;
export declare function Input(alias?: string): PropertyDecorator;
export declare function HostBinding(name?: string): PropertyDecorator;
export declare function HostListener(event: string, args?: string[]): MethodDecorator;
export declare function input<T>(initial?: T, options?: { alias?: string }): () => T;
export declare function output<T>(): { emit(value: T): void };
export declare function computed<T>(fn: () => T): () => T;
export declare function signal<T>(value: T): () => T;
export interface OnChanges { ngOnChanges(changes: unknown): void; }
export type ɵɵFactoryDeclaration<T, CtorDependencies> = unknown;
export type ɵɵDirectiveDeclaration<T, Selector, ExportAs, InputMap, OutputMap, QueryFields, NgContentSelectors, IsStandalone, HostDirectives> = unknown;
export type ɵɵComponentDeclaration<T, Selector, ExportAs, InputMap, OutputMap, QueryFields, NgContentSelectors, IsStandalone, HostDirectives, IsSignal> = unknown;
