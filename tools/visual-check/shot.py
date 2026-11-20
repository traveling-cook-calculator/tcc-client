import os, sys, time
from selenium import webdriver
from selenium.webdriver.firefox.options import Options
from selenium.webdriver.firefox.service import Service
OUT=os.environ.get('VIS_OUT','out')
P='11111111-1111-4111-8111-111111111111'
TOKEN='44444444-4444-4444-8444-444444444444'
def run(jobs):
    o=Options(); o.add_argument('-headless')
    if os.environ.get('FIREFOX'): o.binary_location=os.environ['FIREFOX']
    d=webdriver.Firefox(options=o, service=Service(os.environ.get('GECKODRIVER','geckodriver')))
    for name,w,h,url,wait,actions in jobs:
        d.set_window_size(w,h); d.get(url); 
        try:
            from selenium.webdriver.support.ui import WebDriverWait
            WebDriverWait(d,40).until(lambda x: x.find_elements('css selector',wait))
        except Exception as e: print('timeout',name,e)
        time.sleep(1.5)
        for a in actions: a(d); time.sleep(.6)
        os.makedirs(OUT, exist_ok=True); d.get_full_page_screenshot_as_file(f'{OUT}/{name}.png'); print('ok',name)
    d.quit()
if __name__=='__main__':
    n=int(sys.argv[1]) if len(sys.argv)>1 else 1
    url=f'http://localhost:8080/project/{P}/team/22222222-2222-4222-8222-22222222222{n}#token={TOKEN}'
    run([(f'team{n}_mobile',390,844,url,'h2',[]),(f'team{n}_desktop',1200,900,url,'h2',[])])
