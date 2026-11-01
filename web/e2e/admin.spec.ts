import { test, expect } from '@playwright/test';
const record = { id:'contribution-001', url_pattern:'https://docs.google.com/document/:id/edit', task_category:'compose-draft', actions:['navigate','edit','save'], status:'queued', submitted_at:1793510000 };
test('email login, private layout, and link fragment removal', async ({ page }) => {
    const sent: string[] = [];
    await page.route('**/v1/admin/**', async route => {
        if(route.request().url().endsWith('/session')) return route.fulfill({status:401,json:{}});
        if(route.request().url().endsWith('/start')) { sent.push(route.request().postDataJSON().email); return route.fulfill({status:202,json:{accepted:true}}); }
        return route.fulfill({status:401,json:{}});
    });
    await page.goto('/admin');
    await expect(page.getByRole('heading',{name:'Review contributed memories.'})).toBeVisible();
    await expect(page.getByRole('navigation')).toHaveCount(0);
    await page.getByLabel('Administrator email').fill('admin@example.test');
    await page.getByRole('button',{name:'Email me a sign-in link'}).click();
    await expect(page.getByText("We've received your sign-in link request.")).toBeVisible();
    expect(sent).toEqual(['admin@example.test']);
    await expect(page.getByRole('button',{name:/Send again in \d+s/})).toBeDisabled();
    await page.goto('/admin/verify#token=test-link-secret');
    await expect(page).toHaveURL(/\/admin\/verify$/);
    await expect(page.getByRole('button',{name:'Complete administrator sign-in'})).toBeEnabled();
    expect(await page.evaluate(()=>localStorage.length)).toBe(0);
    await page.getByRole('button',{name:'Complete administrator sign-in'}).click();
    await expect(page.getByRole('alert')).toContainText('This link has expired');
});
test('review list, detail, confirmation, csrf, filters and expired session', async ({page})=>{
    let status='queued'; let generation=0; let expired=false; let decisionCount=0;
    const unsafe='<img src=x onerror="window.adminXss=true">';
    await page.route('**/v1/admin/**', async route=>{
        const url=new URL(route.request().url());
        if(expired) return route.fulfill({status:401,json:{}});
        if(url.pathname.endsWith('/auth/session')) return route.fulfill({json:{csrf_token:'admin-csrf',expires_at:Math.floor(Date.now()/1000)+7200}});
        if(url.pathname.endsWith('/claim')) {
            expect(route.request().headers()['x-csrf-token']).toBe('admin-csrf');
            status='in_review'; generation=1; return route.fulfill({json:{record:{...record,status},review_generation:generation}});
        }
        if(url.pathname.endsWith('/decision')) {
            expect(route.request().headers()['x-csrf-token']).toBe('admin-csrf');
            expect(route.request().postDataJSON()).toEqual({decision:'reject',review_generation:1});
            status='rejected'; decisionCount++; return route.fulfill({json:{...record,status}});
        }
        if(url.pathname.endsWith('/contributions')) return route.fulfill({json:{schema_version:1,counts:{queued:status==='queued'?1:0,in_review:status==='in_review'?1:0,approved:0,rejected:status==='rejected'?1:0},queue_health:{active_jobs:1,oldest_age_secs:20,alert:false},contributions:[{record:{...record,status},review_generation:generation}],decision_history:[]}});
        return route.fulfill({json:{record:{...record,status},review_generation:generation,submitter_account_id:'customer-001',proposed_memory:{l1:{summary:unsafe}},replay:{samples:3,baseline_successes:2,candidate_successes:3,baseline_tool_calls:8,candidate_tool_calls:5}}});
    });
    await page.goto('/admin');
    await expect(page.getByRole('heading',{name:'Contribution review'})).toBeVisible();
    await page.getByRole('button',{name:/docs.google.com/}).click();
    await expect(page.getByRole('heading',{name:'Proposed memory'})).toBeVisible();
    await expect(page.locator('pre')).toContainText('<img src=x');
    expect(await page.evaluate(()=>Reflect.get(window,'adminXss'))).toBeUndefined();
    await page.getByRole('button',{name:'Start review',exact:true}).click();
    await page.getByRole('button',{name:'Reject',exact:true}).click();
    expect(decisionCount).toBe(0);
    await page.getByRole('button',{name:'Confirm',exact:true}).click();
    await expect(page.getByRole('status')).toContainText('Rejection recorded.');
    expect(decisionCount).toBe(1);
    await page.getByRole('searchbox',{name:'Search contributions'}).fill('no-match');
    await expect(page.getByText('No requests match these filters.')).toBeVisible();
    await page.getByRole('searchbox',{name:'Search contributions'}).fill('');
    await expect(page.getByRole('button',{name:/docs.google.com/})).toBeVisible();
    const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
    expect(overflow).toBe(false);
    await page.screenshot({path:'test-results/admin/'+test.info().project.name+'-review.png',fullPage:true});
    expired=true;
    await page.getByRole('button',{name:'Refresh'}).click();
    await expect(page.getByLabel('Administrator email')).toBeVisible();
    await expect(page.getByText('customer-001')).toHaveCount(0);
    await expect(page.getByText(record.url_pattern,{exact:true})).toHaveCount(0);
});
