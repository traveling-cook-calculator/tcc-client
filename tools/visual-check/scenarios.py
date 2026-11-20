import sys, time
"""Usage: python3 scenarios.py states|interact|closed|register  (mock_api.py must be running)."""
from shot import run, P, TOKEN
def T(n): return f'http://localhost:8080/project/{P}/team/22222222-2222-4222-8222-22222222222{n}#token={TOKEN}'
def click_summary(text):
    def f(d):
        for e in d.find_elements('css selector','summary'):
            if text.lower() in e.text.lower(): e.click(); return
    return f
def type_name(d):
    e=d.find_elements('css selector','input[type=text]')[0]; e.send_keys(' X')
def submit_empty(d):
    for b in d.find_elements('css selector','button'):
        if 'erstellen' in b.text.lower() or 'create team' in b.text.lower(): b.click(); return
def scroll_bottom(d): d.execute_script('window.scrollTo(0, document.body.scrollHeight)')
which=sys.argv[1]
if which=='states':
    run([('s2_canceled_m',390,844,T(2),'h2',[]),('s3_review_m',390,844,T(3),'h2',[]),('s4_nomail_m',390,844,T(4),'h2',[])])
elif which=='interact':
    run([('i1_open_m',390,844,T(1),'h2',[click_summary('persönlicher Link'),click_summary('absagen'),click_summary('Adresse ändern')]),
         ('i2_dirty_m',390,844,T(1),'h2',[type_name])])
elif which=='closed':
    run([('c1_closed_m',390,844,T(1),'h2',[]),('c1_closed_d',1200,900,T(1),'h2',[])])
elif which=='register':
    url=f'http://localhost:8080/cook-and-run/{P}/share'
    run([('r1_empty_m',390,844,url,'form, input',[]),('r2_errors_m',390,844,url,'form, input',[submit_empty])])
