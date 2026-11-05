//! Machine values of densities, distribution functions, survival functions and quantiles against reference values
//! (20-digit references, rounded to the nearest double), through the session: every family, both tails, far tails
//! and large parameters.
// the reference values are written as computed (some are close to library constants)
#![allow(clippy::approx_constant, clippy::excessive_precision)]
mod common;
use common::{rel, Sys};

/// (distribution, x, pdf, cdf, survival)
const POINTS: &[(&str, f64, f64, f64, f64)] = &[
    ("normal_distribution(1.5, 2.0)", -5.0, 0.0010145240286498838, 0.000577025042390767, 0.9994229749576092),
    ("normal_distribution(1.5, 2.0)", -0.25, 0.13602749918927176, 0.19078695285251063, 0.8092130471474894),
    ("normal_distribution(1.5, 2.0)", 1.5, 0.19947114020071635, 0.5, 0.5),
    ("normal_distribution(1.5, 2.0)", 4.0, 0.09132454269451096, 0.8943502263331448, 0.10564977366685525),
    ("normal_distribution(1.5, 2.0)", 20.0, 5.250724914985185e-20, 1.0, 1.1224633591327982e-20),
    ("normal_distribution(0, 1)", -30.0, 1.4736461348785476e-196, 4.906713927148187e-198, 1.0),
    ("normal_distribution(0, 1)", -8.0, 5.052271083536892e-15, 6.220960574271784e-16, 0.9999999999999993),
    ("normal_distribution(0, 1)", -1.0, 0.24197072451914334, 0.15865525393145705, 0.8413447460685429),
    ("normal_distribution(0, 1)", 0.0, 0.3989422804014327, 0.5, 0.5),
    ("normal_distribution(0, 1)", 0.5, 0.35206532676429947, 0.6914624612740131, 0.3085375387259869),
    ("normal_distribution(0, 1)", 9.0, 1.0279773571668915e-18, 1.0, 1.1285884059538405e-19),
    ("normal_distribution(0, 1)", 37.0, 2.1200065515246056e-298, 1.0, 5.725571222524577e-300),
    ("log_normal_distribution(0.5, 0.75)", 0.01, 4.6194675451818665e-09, 4.986432222185064e-12, 0.9999999999950135),
    ("log_normal_distribution(0.5, 0.75)", 0.5, 0.30013236790590825, 0.05582021592253323, 0.9441797840774667),
    ("log_normal_distribution(0.5, 0.75)", 1.0, 0.42593067402980295, 0.2524925375469229, 0.7475074624530771),
    ("log_normal_distribution(0.5, 0.75)", 3.0, 0.12894230331808723, 0.787608193817726, 0.21239180618227402),
    ("log_normal_distribution(0.5, 0.75)", 40.0, 1.5782332812525758e-06, 0.9999893988995918, 1.060110040814033e-05),
    ("uniform_distribution([-1.5, 2.5])", -2.0, 0.0, 0.0, 1.0),
    ("uniform_distribution([-1.5, 2.5])", -1.5, 0.25, 0.0, 1.0),
    ("uniform_distribution([-1.5, 2.5])", 0.0, 0.25, 0.375, 0.625),
    ("uniform_distribution([-1.5, 2.5])", 2.5, 0.25, 1.0, 0.0),
    ("uniform_distribution([-1.5, 2.5])", 3.0, 0.0, 1.0, 0.0),
    ("exponential_distribution(2.5)", -1.0, 0.0, 0.0, 1.0),
    ("exponential_distribution(2.5)", 0.0, 2.5, 0.0, 1.0),
    ("exponential_distribution(2.5)", 0.25, 1.3381535712974757, 0.4647385714810098, 0.5352614285189903),
    ("exponential_distribution(2.5)", 2.0, 0.01684486749771367, 0.9932620530009145, 0.006737946999085467),
    ("exponential_distribution(2.5)", 30.0, 6.696592404520194e-33, 1.0, 2.6786369618080778e-33),
    ("gamma_distribution(0.5, 2.0)", 0.001, 12.609356355490783, 0.025227120630039613, 0.9747728793699604),
    ("gamma_distribution(0.5, 2.0)", 0.25, 0.7041306535285989, 0.3829249225480262, 0.6170750774519738),
    ("gamma_distribution(0.5, 2.0)", 1.0, 0.24197072451914334, 0.6826894921370859, 0.3173105078629141),
    ("gamma_distribution(0.5, 2.0)", 5.0, 0.014644982561926487, 0.9746526813225317, 0.025347318677468263),
    ("gamma_distribution(0.5, 2.0)", 60.0, 4.819477793496812e-15, 0.9999999999999906, 9.485737571073848e-15),
    ("gamma_distribution(3.25, 0.5)", 0.1, 0.01718204984919116, 0.0005545364000598685, 0.9994454635999401),
    ("gamma_distribution(3.25, 0.5)", 1.0, 0.5050622478221383, 0.2685244033294908, 0.7314755966705092),
    ("gamma_distribution(3.25, 0.5)", 1.5, 0.46265340013910405, 0.5178777851578407, 0.48212221484215934),
    ("gamma_distribution(3.25, 0.5)", 4.0, 0.02832780970456226, 0.9812121288559896, 0.018787871144010378),
    ("gamma_distribution(3.25, 0.5)", 20.0, 1.3411337296114578e-14, 0.9999999999999929, 7.0947221636589844e-15),
    ("gamma_distribution(250.0, 1.0)", 180.0, 1.8975823899907395e-07, 4.718944298054762e-07, 0.9999995281055702),
    ("gamma_distribution(250.0, 1.0)", 240.0, 0.021393268535884153, 0.2676500698540158, 0.7323499301459842),
    ("gamma_distribution(250.0, 1.0)", 250.0, 0.02522291618453014, 0.508410626968991, 0.491589373031009),
    ("gamma_distribution(250.0, 1.0)", 300.0, 0.0002530471599617795, 0.9986225281224718, 0.001377471877528202),
    ("gamma_distribution(250.0, 1.0)", 400.0, 1.2119890401206978e-16, 0.9999999999999997, 3.1770076592385687e-16),
    ("beta_distribution(0.5, 0.5)", 0.001, 10.070879119947094, 0.020135041633377492, 0.9798649583666225),
    ("beta_distribution(0.5, 0.5)", 0.25, 0.7351051938957227, 0.3333333333333333, 0.6666666666666666),
    ("beta_distribution(0.5, 0.5)", 0.5, 0.6366197723675814, 0.5, 0.5),
    ("beta_distribution(0.5, 0.5)", 0.9, 1.0610329539459689, 0.7951672353008665, 0.20483276469913345),
    ("beta_distribution(0.5, 0.5)", 0.999, 10.070879119947094, 0.9798649583666225, 0.020135041633377492),
    ("beta_distribution(2.5, 7.0)", 0.01, 0.11734253688249621, 0.0004775896614751447, 0.9995224103385248),
    ("beta_distribution(2.5, 7.0)", 0.125, 2.4720522350290053, 0.15921079085805653, 0.8407892091419434),
    ("beta_distribution(2.5, 7.0)", 0.25, 2.7728214114904404, 0.5110304281115532, 0.4889695718884468),
    ("beta_distribution(2.5, 7.0)", 0.75, 0.019764064053564987, 0.9992492204248697, 0.0007507795751302616),
    ("beta_distribution(2.5, 7.0)", 0.96875, 1.1067818000098027e-07, 0.9999999995029071, 4.970928671237124e-10),
    ("beta_distribution(40.0, 60.0)", 0.1, 6.587233158678161e-13, 1.957444819944798e-15, 0.999999999999998),
    ("beta_distribution(40.0, 60.0)", 0.375, 7.28677640794614, 0.3087821967608015, 0.6912178032391985),
    ("beta_distribution(40.0, 60.0)", 0.4, 8.121914499610604, 0.5054370011155946, 0.4945629988844053),
    ("beta_distribution(40.0, 60.0)", 0.5, 1.0410112043172468, 0.9780623532064924, 0.0219376467935076),
    ("beta_distribution(40.0, 60.0)", 0.75, 1.3312044563846848e-11, 0.9999999999999297, 7.028325854284172e-14),
    ("chi_square_distribution(3.0)", 0.01, 0.03969525474770118, 0.0002651650586556098, 0.9997348349413444),
    ("chi_square_distribution(3.0)", 1.0, 0.24197072451914334, 0.1987480430987992, 0.8012519569012008),
    ("chi_square_distribution(3.0)", 3.0, 0.15418032980376928, 0.608374823728911, 0.39162517627108895),
    ("chi_square_distribution(3.0)", 10.0, 0.008500366602520341, 0.9814338645369568, 0.018566135463043233),
    ("chi_square_distribution(3.0)", 100.0, 7.694598626706419e-22, 1.0, 1.554159431389605e-21),
    ("chi_square_distribution(17.5)", 1.0, 5.944650112737112e-08, 7.159212729255086e-09, 0.9999999928407872),
    ("chi_square_distribution(17.5)", 10.0, 0.03713651361571625, 0.08124296554936694, 0.9187570344506331),
    ("chi_square_distribution(17.5)", 17.0, 0.06850930845137218, 0.5111343125021179, 0.4888656874978821),
    ("chi_square_distribution(17.5)", 30.0, 0.008405160960842352, 0.9684802731038185, 0.03151972689618158),
    ("chi_square_distribution(17.5)", 80.0, 2.3358323141944803e-10, 0.9999999994245633, 5.754367119185354e-10),
    ("student_t_distribution(1.0)", -1000.0, 3.1830956787422277e-07, 0.0003183097800805589, 0.9996816902199195),
    ("student_t_distribution(1.0)", -1.0, 0.15915494309189535, 0.25, 0.75),
    ("student_t_distribution(1.0)", 0.0, 0.3183098861837907, 0.5, 0.5),
    ("student_t_distribution(1.0)", 1.5, 0.09794150344116637, 0.8128329581890011, 0.18716704181099883),
    ("student_t_distribution(1.0)", 50.0, 0.0001272730452554141, 0.9936346508990272, 0.006365349100972796),
    ("student_t_distribution(3.0)", -20.0, 2.0368165388191246e-05, 0.00013660162512365586, 0.9998633983748764),
    ("student_t_distribution(3.0)", -2.0, 0.0675096606638929, 0.0696629842794216, 0.9303370157205784),
    ("student_t_distribution(3.0)", 0.25, 0.35270353326442005, 0.5906353887855852, 0.4093646112144148),
    ("student_t_distribution(3.0)", 1.0, 0.206748335783172, 0.8044988905221147, 0.19550110947788532),
    ("student_t_distribution(3.0)", 100.0, 3.305989481302917e-08, 0.9999988977390384, 1.1022609615924555e-06),
    ("student_t_distribution(7.5)", -6.0, 0.00021973874193828113, 0.00020799120350273377, 0.9997920087964972),
    ("student_t_distribution(7.5)", -0.5, 0.33570185429763505, 0.3157054823112272, 0.6842945176887728),
    ("student_t_distribution(7.5)", 0.0, 0.38590073556456983, 0.5, 0.5),
    ("student_t_distribution(7.5)", 2.0, 0.06273648094096242, 0.9585515023509168, 0.041448497649083156),
    ("student_t_distribution(7.5)", 9.0, 1.0739327222260818e-05, 0.9999860535793728, 1.3946420627284908e-05),
    ("student_t_distribution(200.0)", -4.0, 0.00017429293110372268, 4.456547609296719e-05, 0.999955434523907),
    ("student_t_distribution(200.0)", -1.0, 0.241367056087852, 0.15925942395487333, 0.8407405760451266),
    ("student_t_distribution(200.0)", 0.125, 0.3953278794636788, 0.549675448717491, 0.45032455128250903),
    ("student_t_distribution(200.0)", 2.0, 0.054456526677776826, 0.9765734069064645, 0.023426593093535487),
    ("student_t_distribution(200.0)", 6.0, 2.3780745531122035e-08, 0.9999999954424398, 4.557560174405193e-09),
    ("binomial_distribution(20, 0.25)", 0.0, 0.0031712119389339932, 0.0031712119389339932, 0.996828788061066),
    ("binomial_distribution(20, 0.25)", 3.0, 0.13389561519943527, 0.22515604766431352, 0.7748439523356865),
    ("binomial_distribution(20, 0.25)", 5.0, 0.2023311518569244, 0.6171726543871046, 0.38282734561289544),
    ("binomial_distribution(20, 0.25)", 11.0, 0.00300675008475082, 0.9990646084206674, 0.000935391579332645),
    ("binomial_distribution(20, 0.25)", 20.0, 9.094947017729282e-13, 1.0, 0.0),
    ("binomial_distribution(1000, 0.375)", 300.0, 1.1482919068104873e-07, 3.898753487718815e-07, 0.9999996101246512),
    ("binomial_distribution(1000, 0.375)", 360.0, 0.016215554196056338, 0.17185618217680904, 0.828143817823191),
    ("binomial_distribution(1000, 0.375)", 375.0, 0.02605170781725675, 0.5141104221707216, 0.4858895778292785),
    ("binomial_distribution(1000, 0.375)", 420.0, 0.00036146255866114334, 0.9984266356658182, 0.0015733643341817977),
    ("binomial_distribution(1000, 0.375)", 700.0, 2.0919555740723805e-96, 1.0, 7.211610944047113e-97),
    ("poisson_distribution(3.5)", 0.0, 0.0301973834223185, 0.0301973834223185, 0.9698026165776815),
    ("poisson_distribution(3.5)", 2.0, 0.18495897346170082, 0.3208471988621341, 0.6791528011378659),
    ("poisson_distribution(3.5)", 3.0, 0.21578546903865095, 0.536632667900785, 0.463367332099215),
    ("poisson_distribution(3.5)", 8.0, 0.016865264035214874, 0.9901263419438951, 0.009873658056104814),
    ("poisson_distribution(3.5)", 25.0, 7.780808797389331e-14, 0.999999999999988, 1.2024824213822193e-14),
    ("poisson_distribution(500.0)", 400.0, 4.308703456033822e-07, 2.078089610012102e-06, 0.9999979219103899),
    ("poisson_distribution(500.0)", 480.0, 0.012137592474089784, 0.19206157928099474, 0.8079384207190052),
    ("poisson_distribution(500.0)", 500.0, 0.01783826786951178, 0.5118911216987514, 0.4881088783012486),
    ("poisson_distribution(500.0)", 550.0, 0.0015115070495214495, 0.987101779159608, 0.012898220840392022),
    ("poisson_distribution(500.0)", 700.0, 5.592174952790204e-18, 1.0, 1.3745509475966347e-17),
    ("geometric_distribution(0.125)", 0.0, 0.125, 0.125, 0.875),
    ("geometric_distribution(0.125)", 1.0, 0.109375, 0.234375, 0.765625),
    ("geometric_distribution(0.125)", 7.0, 0.04908698797225952, 0.6563910841941833, 0.34360891580581665),
    ("geometric_distribution(0.125)", 30.0, 0.0022758917612543893, 0.9840687576712193, 0.015931242328780727),
    ("geometric_distribution(0.125)", 200.0, 3.1515239904665606e-13, 0.999999999997794, 2.2060667933265923e-12),
];

/// (distribution, q, quantile)
const QUANTILES: &[(&str, f64, f64)] = &[
    ("normal_distribution(1.5, 2.0)", 1.0e-10, -11.222681804808113),
    ("normal_distribution(1.5, 2.0)", 0.01, -3.1526957480816824),
    ("normal_distribution(1.5, 2.0)", 0.3, 0.45119897458391844),
    ("normal_distribution(1.5, 2.0)", 0.5, 1.5),
    ("normal_distribution(1.5, 2.0)", 0.9, 4.063103131089201),
    ("normal_distribution(1.5, 2.0)", 0.999, 7.680464612335627),
    ("normal_distribution(0, 1)", 1.0e-10, -6.361340902404057),
    ("normal_distribution(0, 1)", 0.01, -2.326347874040841),
    ("normal_distribution(0, 1)", 0.3, -0.5244005127080408),
    ("normal_distribution(0, 1)", 0.5, 0.0),
    ("normal_distribution(0, 1)", 0.9, 1.2815515655446004),
    ("normal_distribution(0, 1)", 0.999, 3.0902323061678136),
    ("log_normal_distribution(0.5, 0.75)", 1.0e-10, 0.013967729066474905),
    ("log_normal_distribution(0.5, 0.75)", 0.01, 0.2880097614403042),
    ("log_normal_distribution(0.5, 0.75)", 0.3, 1.1125999964507098),
    ("log_normal_distribution(0.5, 0.75)", 0.5, 1.6487212707001282),
    ("log_normal_distribution(0.5, 0.75)", 0.9, 4.310973178737611),
    ("log_normal_distribution(0.5, 0.75)", 0.999, 16.737876909454652),
    ("uniform_distribution([-1.5, 2.5])", 1.0e-10, -1.4999999996),
    ("uniform_distribution([-1.5, 2.5])", 0.01, -1.46),
    ("uniform_distribution([-1.5, 2.5])", 0.3, -0.3),
    ("uniform_distribution([-1.5, 2.5])", 0.5, 0.5),
    ("uniform_distribution([-1.5, 2.5])", 0.9, 2.1),
    ("uniform_distribution([-1.5, 2.5])", 0.999, 2.496),
    ("exponential_distribution(2.5)", 1.0e-10, 4.0000000002e-11),
    ("exponential_distribution(2.5)", 0.01, 0.004020134341400576),
    ("exponential_distribution(2.5)", 0.3, 0.14266997757549296),
    ("exponential_distribution(2.5)", 0.5, 0.2772588722239781),
    ("exponential_distribution(2.5)", 0.9, 0.9210340371976182),
    ("exponential_distribution(2.5)", 0.999, 2.763102111592855),
    ("gamma_distribution(0.5, 2.0)", 1.0e-10, 1.5707963267948965e-20),
    ("gamma_distribution(0.5, 2.0)", 0.01, 0.00015708785790970197),
    ("gamma_distribution(0.5, 2.0)", 0.3, 0.14847186183254546),
    ("gamma_distribution(0.5, 2.0)", 0.5, 0.4549364231195728),
    ("gamma_distribution(0.5, 2.0)", 0.9, 2.7055434540954146),
    ("gamma_distribution(0.5, 2.0)", 0.999, 10.827566170662733),
    ("gamma_distribution(3.25, 0.5)", 1.0e-10, 0.0008030895368033275),
    ("gamma_distribution(3.25, 0.5)", 0.01, 0.26250492674268494),
    ("gamma_distribution(3.25, 0.5)", 0.3, 1.0619256916489315),
    ("gamma_distribution(3.25, 0.5)", 0.5, 1.461717279446336),
    ("gamma_distribution(3.25, 0.5)", 0.9, 2.83368630963864),
    ("gamma_distribution(3.25, 0.5)", 0.999, 5.849602981778768),
    ("gamma_distribution(250.0, 1.0)", 1.0e-10, 162.17334651003375),
    ("gamma_distribution(250.0, 1.0)", 0.01, 214.69377138728984),
    ("gamma_distribution(250.0, 1.0)", 0.3, 241.47309714489376),
    ("gamma_distribution(250.0, 1.0)", 0.5, 249.6667457944369),
    ("gamma_distribution(250.0, 1.0)", 0.9, 270.4651541049084),
    ("gamma_distribution(250.0, 1.0)", 0.999, 301.7230028975888),
    ("beta_distribution(0.5, 0.5)", 1.0e-10, 2.4674011002723397e-20),
    ("beta_distribution(0.5, 0.5)", 0.01, 0.0002467198171342215),
    ("beta_distribution(0.5, 0.5)", 0.3, 0.20610737385376343),
    ("beta_distribution(0.5, 0.5)", 0.5, 0.5),
    ("beta_distribution(0.5, 0.5)", 0.9, 0.9755282581475768),
    ("beta_distribution(0.5, 0.5)", 0.999, 0.999997532600929),
    ("beta_distribution(2.5, 7.0)", 1.0e-10, 2.093793608200994e-05),
    ("beta_distribution(2.5, 7.0)", 0.01, 0.035265857309230306),
    ("beta_distribution(2.5, 7.0)", 0.3, 0.17680146270511068),
    ("beta_distribution(2.5, 7.0)", 0.5, 0.24603732885541796),
    ("beta_distribution(2.5, 7.0)", 0.9, 0.4517268017641158),
    ("beta_distribution(2.5, 7.0)", 0.999, 0.738849999220434),
    ("beta_distribution(40.0, 60.0)", 1.0e-10, 0.13988245269605584),
    ("beta_distribution(40.0, 60.0)", 0.01, 0.2902185859792949),
    ("beta_distribution(40.0, 60.0)", 0.3, 0.3737876219047363),
    ("beta_distribution(40.0, 60.0)", 0.5, 0.3993307426981328),
    ("beta_distribution(40.0, 60.0)", 0.9, 0.4630998664764221),
    ("beta_distribution(40.0, 60.0)", 0.999, 0.5538117580359364),
    ("chi_square_distribution(3.0)", 1.0e-10, 5.20939762143448e-07),
    ("chi_square_distribution(3.0)", 0.01, 0.11483180189911704),
    ("chi_square_distribution(3.0)", 0.3, 1.4236522430352796),
    ("chi_square_distribution(3.0)", 0.5, 2.365973884375338),
    ("chi_square_distribution(3.0)", 0.9, 6.2513886311703235),
    ("chi_square_distribution(3.0)", 0.999, 16.266236196238133),
    ("chi_square_distribution(17.5)", 1.0e-10, 0.6014134537483539),
    ("chi_square_distribution(17.5)", 0.01, 6.709946417387386),
    ("chi_square_distribution(17.5)", 0.3, 13.984928204376232),
    ("chi_square_distribution(17.5)", 0.5, 16.838038216438118),
    ("chi_square_distribution(17.5)", 0.9, 25.380043365585234),
    ("chi_square_distribution(17.5)", 0.999, 41.55318099258925),
    ("student_t_distribution(1.0)", 1.0e-10, -3183098861.837907),
    ("student_t_distribution(1.0)", 0.01, -31.82051595377396),
    ("student_t_distribution(1.0)", 0.3, -0.7265425280053609),
    ("student_t_distribution(1.0)", 0.5, 0.0),
    ("student_t_distribution(1.0)", 0.9, 3.0776835371752536),
    ("student_t_distribution(1.0)", 0.999, 318.30883898555044),
    ("student_t_distribution(3.0)", 1.0e-10, -2225.7692846830932),
    ("student_t_distribution(3.0)", 0.01, -4.5407028585681335),
    ("student_t_distribution(3.0)", 0.3, -0.5843897274398187),
    ("student_t_distribution(3.0)", 0.5, 0.0),
    ("student_t_distribution(3.0)", 0.9, 1.6377443536962102),
    ("student_t_distribution(3.0)", 0.999, 10.214531852407386),
    ("student_t_distribution(7.5)", 1.0e-10, -45.36104424133951),
    ("student_t_distribution(7.5)", 0.01, -2.943099323406722),
    ("student_t_distribution(7.5)", 0.3, -0.5474128775812612),
    ("student_t_distribution(7.5)", 0.5, 0.0),
    ("student_t_distribution(7.5)", 0.9, 1.4052118464159529),
    ("student_t_distribution(7.5)", 0.999, 4.6303190940341015),
    ("student_t_distribution(200.0)", 1.0e-10, -6.706200879452119),
    ("student_t_distribution(200.0)", 0.01, -2.3451370822594684),
    ("student_t_distribution(200.0)", 0.3, -0.5252373340155535),
    ("student_t_distribution(200.0)", 0.5, 0.0),
    ("student_t_distribution(200.0)", 0.9, 1.285798793994801),
    ("student_t_distribution(200.0)", 0.999, 3.1314798142211213),
    ("binomial_distribution(20, 0.25)", 1.0e-10, 0.0),
    ("binomial_distribution(20, 0.25)", 0.01, 1.0),
    ("binomial_distribution(20, 0.25)", 0.3, 4.0),
    ("binomial_distribution(20, 0.25)", 0.5, 5.0),
    ("binomial_distribution(20, 0.25)", 0.9, 8.0),
    ("binomial_distribution(20, 0.25)", 0.999, 11.0),
    ("binomial_distribution(1000, 0.375)", 1.0e-10, 280.0),
    ("binomial_distribution(1000, 0.375)", 0.01, 340.0),
    ("binomial_distribution(1000, 0.375)", 0.3, 367.0),
    ("binomial_distribution(1000, 0.375)", 0.5, 375.0),
    ("binomial_distribution(1000, 0.375)", 0.9, 395.0),
    ("binomial_distribution(1000, 0.375)", 0.999, 423.0),
    ("poisson_distribution(3.5)", 1.0e-10, 0.0),
    ("poisson_distribution(3.5)", 0.01, 0.0),
    ("poisson_distribution(3.5)", 0.3, 2.0),
    ("poisson_distribution(3.5)", 0.5, 3.0),
    ("poisson_distribution(3.5)", 0.9, 6.0),
    ("poisson_distribution(3.5)", 0.999, 11.0),
    ("poisson_distribution(500.0)", 1.0e-10, 365.0),
    ("poisson_distribution(500.0)", 0.01, 449.0),
    ("poisson_distribution(500.0)", 0.3, 488.0),
    ("poisson_distribution(500.0)", 0.5, 500.0),
    ("poisson_distribution(500.0)", 0.9, 529.0),
    ("poisson_distribution(500.0)", 0.999, 571.0),
    ("geometric_distribution(0.125)", 1.0e-10, 0.0),
    ("geometric_distribution(0.125)", 0.01, 0.0),
    ("geometric_distribution(0.125)", 0.3, 2.0),
    ("geometric_distribution(0.125)", 0.5, 5.0),
    ("geometric_distribution(0.125)", 0.9, 17.0),
    ("geometric_distribution(0.125)", 0.999, 51.0),
];

/// The largest relative error allowed: the machine paths are accurate to a few ulps in the tested ranges except
/// where the condition of the problem itself is larger (far tails of cdf at large x, where the rounding of x
/// costs about x^2 ulps for the normal family, or x ulps for large shapes).
const TOL: f64 = 2e-14;

#[test]
fn densities_and_distribution_functions() {
    let mut s = Sys::new();
    let mut worst = (0.0f64, String::new());
    for &(d, x, pdf, cdf, sf) in POINTS {
        let src = format!("[pdf({d}, {x:?}), cdf({d}, {x:?}), survival({d}, {x:?})]");
        let got = s.numbers(&src);
        assert_eq!(got.len(), 3, "{src}");
        for (k, (&g, &w)) in got.iter().zip(&[pdf, cdf, sf]).enumerate() {
            let e = rel(g, w);
            if e > worst.0 {
                worst = (e, format!("{src} [{k}]: {g:e} vs {w:e}"));
            }
            assert!(e <= TOL, "{src} [{k}]: {g:e}, want {w:e} (relative error {e:.2e})");
        }
    }
    println!("largest relative error: {:.2e} at {}", worst.0, worst.1);
}

#[test]
fn quantiles() {
    let mut s = Sys::new();
    let mut worst = (0.0f64, String::new());
    for &(d, q, want) in QUANTILES {
        let src = format!("quantile({d}, {q:?})");
        let got = s.number(&src);
        let e = rel(got, want);
        if e > worst.0 {
            worst = (e, format!("{src}: {got:e} vs {want:e}"));
        }
        assert!(e <= TOL, "{src}: {got:e}, want {want:e} (relative error {e:.2e})");
    }
    println!("largest relative error: {:.2e} at {}", worst.0, worst.1);
}

#[test]
fn quantiles_invert_cdf_on_packed_grids() {
    let mut s = Sys::new();
    let ds = [
        "normal_distribution(-2.0, 0.5)",
        "log_normal_distribution(0.0, 1.5)",
        "gamma_distribution(0.75, 3.0)",
        "gamma_distribution(40.0, 0.25)",
        "beta_distribution(0.5, 3.0)",
        "beta_distribution(12.0, 12.0)",
        "chi_square_distribution(2.5)",
        "student_t_distribution(4.5)",
        "exponential_distribution(0.1)",
        "uniform_distribution([2.0, 3.0])",
    ];
    for d in ds {
        let qs: Vec<String> = (1..100).map(|k| format!("{:?}", k as f64 / 100.0)).collect();
        let src = format!("cdf({d}, quantile({d}, [{}]))", qs.join(", "));
        let back = s.numbers(&src);
        for (k, b) in back.iter().enumerate() {
            let q = (k + 1) as f64 / 100.0;
            assert!(rel(*b, q) < 1e-13, "{d}: q = {q}, cdf(quantile) = {b}");
        }
    }
}
