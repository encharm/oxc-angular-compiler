// NgIf and NgTemplateOutlet as @angular/common declares them.
import * as i0 from '@angular/core';
export declare class NgIf<T = unknown> {
    set ngIf(condition: T);
    set ngIfThen(templateRef: unknown);
    set ngIfElse(templateRef: unknown);
    static ɵfac: i0.ɵɵFactoryDeclaration<NgIf<any>, never>;
    static ɵdir: i0.ɵɵDirectiveDeclaration<NgIf<any>, "[ngIf]", never, { "ngIf": { "alias": "ngIf"; "required": false; }; "ngIfThen": { "alias": "ngIfThen"; "required": false; }; "ngIfElse": { "alias": "ngIfElse"; "required": false; }; }, {}, never, never, true, never>;
}
export declare class NgTemplateOutlet<C = unknown> {
    ngTemplateOutletContext: C | null;
    ngTemplateOutlet: unknown;
    ngTemplateOutletInjector: unknown;
    static ɵfac: i0.ɵɵFactoryDeclaration<NgTemplateOutlet<any>, never>;
    static ɵdir: i0.ɵɵDirectiveDeclaration<NgTemplateOutlet<any>, "[ngTemplateOutlet]", never, { "ngTemplateOutletContext": { "alias": "ngTemplateOutletContext"; "required": false; }; "ngTemplateOutlet": { "alias": "ngTemplateOutlet"; "required": false; }; "ngTemplateOutletInjector": { "alias": "ngTemplateOutletInjector"; "required": false; }; }, {}, never, never, true, never>;
}
